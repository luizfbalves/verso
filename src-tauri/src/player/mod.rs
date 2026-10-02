use std::fmt;

pub mod macos;

#[cfg(windows)]
pub mod windows;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub is_playing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackKey {
    pub artist: String,
    pub title: String,
    pub duration_s: u64,
}

impl TrackKey {
    pub fn id(&self) -> String {
        format!("{}|{}|{}", self.artist, self.title, self.duration_s)
    }
}

impl NowPlaying {
    pub fn key(&self) -> TrackKey {
        TrackKey {
            artist: self.artist.clone(),
            title: self.title.clone(),
            duration_s: (self.duration_ms + 500) / 1000,
        }
    }

    pub fn display_title(&self) -> String {
        format!("{} — {}", self.title, self.artist)
    }
}

#[derive(Debug)]
pub struct PlayerError(pub String);

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub trait Player: Send + Sync {
    fn now_playing(&self) -> Result<Option<NowPlaying>, PlayerError>;
}

/// Tempo decorrido desde um `DateTime.UniversalTime` do WinRT (ticks de 100 ns desde 1601-01-01 UTC).
pub fn smtc_elapsed_ms(universal_time: i64, now_unix_ms: u64) -> u64 {
    const EPOCH_DIFF_MS: i64 = 11_644_473_600_000;
    let then_unix_ms = universal_time / 10_000 - EPOCH_DIFF_MS;
    (now_unix_ms as i64 - then_unix_ms).max(0) as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtcSource {
    Spotify,
    Browser,
}

/// Classifica uma sessão do SMTC pelo AppUserModelId. Navegadores entram por causa do YouTube Music
/// (aba ou PWA); o AUMID do Firefox é um hash fixo em vez do nome.
pub fn smtc_source(app_id: &str) -> Option<SmtcSource> {
    const BROWSERS: [&str; 7] = ["chrome", "msedge", "firefox", "308046b0af4a39cb", "brave", "opera", "vivaldi"];
    let id = app_id.to_lowercase();
    if id.contains("spotify") {
        Some(SmtcSource::Spotify)
    } else if BROWSERS.iter().any(|b| id.contains(b)) {
        Some(SmtcSource::Browser)
    } else {
        None
    }
}

/// Escolhe a sessão a sincronizar: a que está tocando ganha, e no empate o Spotify ganha.
/// De navegador só vale faixa com artista e duração: sem duração não dá para achar a letra nem
/// sincronizar. O álbum não é exigido porque o YouTube Music o deixa vazio em vídeos e em muitas
/// faixas; a busca do LRCLIB casa pela duração, então um vídeo comum só fica sem letra.
pub fn pick_smtc(candidates: Vec<(SmtcSource, NowPlaying)>) -> Option<NowPlaying> {
    candidates
        .into_iter()
        .filter(|(src, np)| {
            *src == SmtcSource::Spotify || (!np.artist.trim().is_empty() && np.duration_ms > 0)
        })
        .min_by_key(|(src, np)| (!np.is_playing, *src != SmtcSource::Spotify))
        .map(|(_, np)| np)
}

#[cfg(target_os = "macos")]
pub fn system_player() -> std::sync::Arc<dyn Player> {
    std::sync::Arc::new(macos::MacSpotifyPlayer)
}

#[cfg(windows)]
pub fn system_player() -> std::sync::Arc<dyn Player> {
    std::sync::Arc::new(windows::WinSmtcPlayer)
}

#[cfg(not(any(target_os = "macos", windows)))]
compile_error!("plataforma não suportada");

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-01-01T00:00:00Z em ms Unix e em ticks de 100 ns desde 1601.
    const UNIX_MS: u64 = 1_767_225_600_000;
    const WIN_TICKS: i64 = (1_767_225_600_000 + 11_644_473_600_000) * 10_000;

    #[test]
    fn smtc_elapsed() {
        assert_eq!(smtc_elapsed_ms(WIN_TICKS, UNIX_MS), 0);
        assert_eq!(smtc_elapsed_ms(WIN_TICKS, UNIX_MS + 2_500), 2_500);
        // relógio "voltou": nunca negativo
        assert_eq!(smtc_elapsed_ms(WIN_TICKS, UNIX_MS - 1_000), 0);
    }

    fn np(title: &str, album: &str, duration_ms: u64, is_playing: bool) -> NowPlaying {
        NowPlaying {
            title: title.into(),
            artist: "Artista".into(),
            album: album.into(),
            duration_ms,
            position_ms: 0,
            is_playing,
        }
    }

    #[test]
    fn smtc_source_por_app_id() {
        assert_eq!(smtc_source("Spotify.exe"), Some(SmtcSource::Spotify));
        assert_eq!(smtc_source("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), Some(SmtcSource::Spotify));
        assert_eq!(smtc_source("Chrome"), Some(SmtcSource::Browser));
        assert_eq!(smtc_source("MSEdge"), Some(SmtcSource::Browser));
        assert_eq!(smtc_source("308046B0AF4A39CB"), Some(SmtcSource::Browser));
        assert_eq!(smtc_source("Chrome._crx_cinhimbnkkaeohfgghhklpknlkffjgod"), Some(SmtcSource::Browser));
        assert_eq!(smtc_source("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), None);
    }

    #[test]
    fn pick_smtc_prefere_quem_toca_e_depois_spotify() {
        use SmtcSource::*;
        let got = pick_smtc(vec![(Spotify, np("pausada", "A", 1, false)), (Browser, np("ytm", "B", 1, true))]);
        assert_eq!(got.unwrap().title, "ytm");
        let got = pick_smtc(vec![(Browser, np("ytm", "B", 1, true)), (Spotify, np("spotify", "A", 1, true))]);
        assert_eq!(got.unwrap().title, "spotify");
        let got = pick_smtc(vec![(Browser, np("ytm", "B", 1, false)), (Spotify, np("spotify", "A", 1, false))]);
        assert_eq!(got.unwrap().title, "spotify");
    }

    #[test]
    fn pick_smtc_ignora_navegador_sem_duracao() {
        use SmtcSource::*;
        assert_eq!(pick_smtc(vec![(Browser, np("sem timeline", "B", 0, true))]), None);
        let got = pick_smtc(vec![(Browser, np("sem timeline", "B", 0, true)), (Spotify, np("spotify", "", 0, false))]);
        assert_eq!(got.unwrap().title, "spotify");
    }

    #[test]
    fn pick_smtc_aceita_navegador_sem_album() {
        // O YouTube Music manda o álbum vazio em vídeos e em muitas faixas.
        let got = pick_smtc(vec![(SmtcSource::Browser, np("ytm", "", 217_741, true))]);
        assert_eq!(got.unwrap().title, "ytm");
    }
}
