use super::{TranslateError, Translated, Translator};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Mesmo limite do proxy (proxy/src/index.js): acima disso ele responde 400.
const BATCH: usize = 400;

#[derive(Serialize)]
struct Req<'a> {
    to: &'a str,
    lines: Vec<&'a str>,
}

#[derive(Deserialize)]
struct Resp {
    lines: Vec<String>,
    source: String,
}

pub struct ProxyTranslator {
    http: reqwest::Client,
    base: String,
    key: String,
}

/// hex(HMAC-SHA256(chave, "<ts>.<corpo>")), conferido pelo proxy (proxy/src/index.js).
fn signature(key: &str, ts: u64, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC aceita qualquer chave");
    mac.update(format!("{ts}.").as_bytes());
    mac.update(body);
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn net(e: reqwest::Error) -> TranslateError {
    TranslateError::Network(e.to_string())
}

impl ProxyTranslator {
    pub fn new(base: &str, timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("Verso/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("cliente HTTP");
        Self { http, base: base.trim_end_matches('/').to_string(), key: super::SIGNING_KEY.to_string() }
    }

    #[cfg(test)]
    fn with_key(mut self, key: &str) -> Self {
        self.key = key.to_string();
        self
    }
}

#[async_trait]
impl Translator for ProxyTranslator {
    /// `target` é o código de idioma da Azure (ex.: "pt").
    async fn translate(&self, lines: &[String], target: &str) -> Result<Translated, TranslateError> {
        let idx: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, _)| i)
            .collect();
        let mut out = vec![String::new(); lines.len()];
        let mut source_lang = String::new();
        for chunk in idx.chunks(BATCH) {
            let req = Req { to: target, lines: chunk.iter().map(|&i| lines[i].as_str()).collect() };
            // Assina exatamente os bytes enviados, por isso serializa aqui em vez de usar .json().
            let body = serde_json::to_vec(&req).map_err(|e| TranslateError::Unexpected(e.to_string()))?;
            let mut post = self
                .http
                .post(format!("{}/v1/translate", self.base))
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            if !self.key.is_empty() {
                let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                post = post.header("x-verso-ts", ts.to_string()).header("x-verso-sig", signature(&self.key, ts, &body));
            }
            let resp = post.body(body).send().await.map_err(net)?;
            match resp.status().as_u16() {
                503 => return Err(TranslateError::QuotaExceeded),
                // 429 (limite por IP) e 5xx passageiros: não guarda nada, tenta de novo na próxima faixa.
                s if !(200..300).contains(&s) => return Err(TranslateError::Network(format!("HTTP {s}"))),
                _ => {}
            }
            let body: Resp = resp.json().await.map_err(net)?;
            if body.lines.len() != chunk.len() {
                return Err(TranslateError::Unexpected(format!(
                    "proxy devolveu {} de {} linhas",
                    body.lines.len(),
                    chunk.len()
                )));
            }
            if source_lang.is_empty() {
                source_lang = body.source;
            }
            for (&i, text) in chunk.iter().zip(body.lines) {
                out[i] = text;
            }
        }
        Ok(Translated { lines: out, source_lang })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::{json, Value};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    /// Responde "tr:<texto>" para cada linha recebida, com idioma de origem fixo.
    pub(crate) struct Echo(pub &'static str);
    impl Respond for Echo {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            let v: Value = serde_json::from_slice(&req.body).unwrap();
            let lines: Vec<String> =
                v["lines"].as_array().unwrap().iter().map(|t| format!("tr:{}", t.as_str().unwrap())).collect();
            ResponseTemplate::new(200).set_body_json(json!({ "lines": lines, "source": self.0 }))
        }
    }

    fn client(s: &MockServer) -> ProxyTranslator {
        ProxyTranslator::new(&s.uri(), Duration::from_secs(2))
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn preserves_empty_lines_and_indices() {
        let s = MockServer::start().await;
        Mock::given(method("POST")).and(path("/v1/translate")).respond_with(Echo("ja")).expect(1).mount(&s).await;
        let t = client(&s).translate(&strs(&["um", "", "  ", "dois"]), "pt").await.unwrap();
        assert_eq!(t.lines, strs(&["tr:um", "", "", "tr:dois"]));
        assert_eq!(t.source_lang, "ja");
        let req = &s.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body, json!({ "to": "pt", "lines": ["um", "dois"] }));
        assert!(req.headers["user-agent"].to_str().unwrap().starts_with("Verso/"));
    }

    #[test]
    fn signature_matches_known_vector() {
        // Mesmo cálculo do Worker: HMAC-SHA256("segredo", "1700000000.{}").
        assert_eq!(
            signature("segredo", 1_700_000_000, b"{}"),
            "28d73844c84580182772a1e98a60aeb8f04184b1bef0d4e147cfa59ebf0bfedc"
        );
    }

    #[tokio::test]
    async fn signs_when_key_is_set() {
        let s = MockServer::start().await;
        Mock::given(path("/v1/translate")).respond_with(Echo("ja")).mount(&s).await;
        client(&s).with_key("segredo").translate(&strs(&["a"]), "pt").await.unwrap();
        let req = &s.received_requests().await.unwrap()[0];
        let ts: u64 = req.headers["x-verso-ts"].to_str().unwrap().parse().unwrap();
        assert_eq!(req.headers["x-verso-sig"].to_str().unwrap(), signature("segredo", ts, &req.body));
        assert_eq!(req.headers["content-type"], "application/json");
    }

    #[tokio::test]
    async fn no_key_no_signature() {
        let s = MockServer::start().await;
        Mock::given(path("/v1/translate")).respond_with(Echo("ja")).mount(&s).await;
        client(&s).with_key("").translate(&strs(&["a"]), "pt").await.unwrap();
        let req = &s.received_requests().await.unwrap()[0];
        assert!(!req.headers.contains_key("x-verso-sig"));
    }

    #[tokio::test]
    async fn splits_into_batches() {
        let s = MockServer::start().await;
        Mock::given(path("/v1/translate")).respond_with(Echo("ja")).expect(2).mount(&s).await;
        let input: Vec<String> = (0..450).map(|i| format!("linha {i}")).collect();
        let t = client(&s).translate(&input, "pt").await.unwrap();
        assert_eq!(t.lines.len(), 450);
        assert_eq!(t.lines[449], "tr:linha 449");
    }

    #[tokio::test]
    async fn all_empty_makes_no_request() {
        let s = MockServer::start().await;
        Mock::given(path("/v1/translate")).respond_with(Echo("ja")).expect(0).mount(&s).await;
        let t = client(&s).translate(&strs(&["", ""]), "pt").await.unwrap();
        assert_eq!(t.lines, strs(&["", ""]));
    }

    #[tokio::test]
    async fn maps_status_codes() {
        for (code, want_quota) in [(503, true), (429, false), (502, false)] {
            let s = MockServer::start().await;
            Mock::given(path("/v1/translate")).respond_with(ResponseTemplate::new(code)).mount(&s).await;
            let r = client(&s).translate(&strs(&["a"]), "pt").await;
            if want_quota {
                assert_eq!(r, Err(TranslateError::QuotaExceeded));
            } else {
                assert!(matches!(r, Err(TranslateError::Network(_))), "{code}: {r:?}");
            }
        }
    }

    #[tokio::test]
    async fn timeout_is_network_error() {
        let s = MockServer::start().await;
        Mock::given(path("/v1/translate"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(500)))
            .mount(&s)
            .await;
        let c = ProxyTranslator::new(&s.uri(), Duration::from_millis(100));
        assert!(matches!(c.translate(&strs(&["a"]), "pt").await, Err(TranslateError::Network(_))));
    }

    #[tokio::test]
    async fn count_mismatch_is_error() {
        let s = MockServer::start().await;
        Mock::given(path("/v1/translate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"lines": ["só uma"], "source": "ja"})))
            .mount(&s)
            .await;
        let r = client(&s).translate(&strs(&["a", "b"]), "pt").await;
        assert!(matches!(r, Err(TranslateError::Unexpected(_))));
    }
}
