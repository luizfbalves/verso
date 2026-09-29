import assert from "node:assert/strict";
import { test } from "node:test";
import { dominant, handle, invalid } from "../src/index.js";

class FakeKV {
  constructor() {
    this.data = new Map();
  }
  async get(k, type) {
    const v = this.data.get(k);
    if (v === undefined) return null;
    return type === "json" ? JSON.parse(v) : v;
  }
  async put(k, v) {
    this.data.set(k, v);
  }
}

/** Azure falsa: traduz para "tr:<texto>" e detecta o idioma fixo. */
function azure(lang = "ja") {
  const calls = [];
  const fn = async (url, init) => {
    calls.push({ url, init });
    const body = JSON.parse(init.body);
    return Response.json(
      body.map((b) => ({ detectedLanguage: { language: lang, score: 1 }, translations: [{ text: `tr:${b.Text}`, to: "pt" }] })),
    );
  };
  fn.calls = calls;
  return fn;
}

const KEY = "segredo";
const env = (extra = {}) => ({ CACHE: new FakeKV(), AZURE_KEY: "k", AZURE_REGION: "brazilsouth", SIGNING_KEY: KEY, ...extra });

async function sign(key, ts, raw) {
  const enc = new TextEncoder();
  const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", k, enc.encode(`${ts}.${raw}`)));
  return [...mac].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Pedido assinado com KEY no horário `now` (o mesmo que vai para `handle`). */
const req = async (body, { path = "/v1/translate", method = "POST", ip = "1.2.3.4", now = new Date() } = {}) => {
  const raw = method === "POST" ? JSON.stringify(body) : undefined;
  const ts = Math.floor(now.getTime() / 1000);
  const sig = raw === undefined ? "" : await sign(KEY, ts, raw);
  return new Request(`https://proxy.test${path}`, {
    method,
    headers: { "content-type": "application/json", "cf-connecting-ip": ip, "x-verso-ts": String(ts), "x-verso-sig": sig },
    body: raw,
  });
};

test("traduz e manda chave e região para a Azure", async () => {
  const e = env();
  const f = azure();
  const r = await handle(await req({ to: "pt", lines: ["a", "b"] }), e, { fetch: f });
  assert.equal(r.status, 200);
  assert.deepEqual(await r.json(), { lines: ["tr:a", "tr:b"], source: "ja" });
  assert.equal(f.calls.length, 1);
  assert.match(f.calls[0].url, /to=pt$/);
  assert.equal(f.calls[0].init.headers["Ocp-Apim-Subscription-Key"], "k");
  assert.equal(f.calls[0].init.headers["Ocp-Apim-Subscription-Region"], "brazilsouth");
});

test("segunda chamada igual vem do cache, sem Azure", async () => {
  const e = env();
  const f = azure();
  await handle(await req({ to: "pt", lines: ["a"] }), e, { fetch: f });
  const r = await handle(await req({ to: "pt", lines: ["a"] }, { ip: "9.9.9.9" }), e, { fetch: f });
  assert.equal(r.status, 200);
  assert.deepEqual(await r.json(), { lines: ["tr:a"], source: "ja" });
  assert.equal(f.calls.length, 1);
});

test("cache separa idiomas de destino", async () => {
  const e = env();
  const f = azure();
  await handle(await req({ to: "pt", lines: ["a"] }), e, { fetch: f });
  await handle(await req({ to: "en", lines: ["a"] }), e, { fetch: f });
  assert.equal(f.calls.length, 2);
});

test("teto mensal devolve 503 quota_exceeded sem chamar a Azure", async () => {
  const e = env({ MONTHLY_BUDGET: "5" });
  const f = azure();
  const now = new Date("2026-09-10T12:00:00Z");
  assert.equal((await handle(await req({ to: "pt", lines: ["abc"] }, { now }), e, { fetch: f, now })).status, 200);
  const r = await handle(await req({ to: "pt", lines: ["xyz"] }, { now }), e, { fetch: f, now });
  assert.equal(r.status, 503);
  assert.deepEqual(await r.json(), { error: "quota_exceeded" });
  assert.equal(f.calls.length, 1);
  // Mês novo, contador novo.
  const oct = new Date("2026-10-01T00:00:00Z");
  const next = await handle(await req({ to: "pt", lines: ["xyz"] }, { now: oct }), e, { fetch: f, now: oct });
  assert.equal(next.status, 200);
});

test("limite diário por IP devolve 429, outro IP segue", async () => {
  const e = env({ IP_DAILY_CHARS: "5" });
  const f = azure();
  assert.equal((await handle(await req({ to: "pt", lines: ["abc"] }), e, { fetch: f })).status, 200);
  assert.equal((await handle(await req({ to: "pt", lines: ["xyz"] }), e, { fetch: f })).status, 429);
  assert.equal((await handle(await req({ to: "pt", lines: ["xyz"] }, { ip: "5.6.7.8" }), e, { fetch: f })).status, 200);
});

test("rate limiter do Cloudflare bloqueia antes de tudo", async () => {
  const e = env({ LIMITER: { limit: async () => ({ success: false }) } });
  const f = azure();
  assert.equal((await handle(await req({ to: "pt", lines: ["a"] }), e, { fetch: f })).status, 429);
  assert.equal(f.calls.length, 0);
});

test("cota da Azure esgotada (403001) vira 503 quota_exceeded", async () => {
  const f = async () => Response.json({ error: { code: 403001, message: "free quota" } }, { status: 403 });
  const r = await handle(await req({ to: "pt", lines: ["a"] }), env(), { fetch: f });
  assert.equal(r.status, 503);
  assert.deepEqual(await r.json(), { error: "quota_exceeded" });
});

test("outros erros da Azure viram 502 e não entram no cache", async () => {
  const e = env();
  const bad = async () => Response.json({ error: { code: 401000 } }, { status: 401 });
  assert.equal((await handle(await req({ to: "pt", lines: ["a"] }), e, { fetch: bad })).status, 502);
  const f = azure();
  assert.equal((await handle(await req({ to: "pt", lines: ["a"] }), e, { fetch: f })).status, 200);
  assert.equal(f.calls.length, 1);
});

test("rotas e métodos errados", async () => {
  assert.equal((await handle(await req({}, { path: "/" }), env())).status, 404);
  assert.equal((await handle(await req(null, { method: "GET" }), env())).status, 405);
});

test("validação da entrada", () => {
  assert.equal(invalid({ to: "pt", lines: ["ok"] }), null);
  assert.ok(invalid({ to: "xx", lines: ["ok"] }));
  assert.ok(invalid({ to: "pt", lines: [] }));
  assert.ok(invalid({ to: "pt", lines: [""] }));
  assert.ok(invalid({ to: "pt", lines: [42] }));
  assert.ok(invalid({ to: "pt", lines: ["x".repeat(301)] }));
  assert.ok(invalid({ to: "pt", lines: Array(401).fill("x") }));
  assert.ok(invalid({ to: "pt", lines: Array(40).fill("x".repeat(300)) }));
  assert.ok(invalid(null));
});

test("idioma de origem é o mais frequente", () => {
  assert.equal(dominant(["en", "ja", "ja", undefined]), "ja");
  assert.equal(dominant([]), "");
});

const signedReq = (raw, headers) =>
  new Request("https://proxy.test/v1/translate", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": "1.2.3.4", ...headers },
    body: raw,
  });

test("só passa pedido assinado e dentro da janela", async () => {
  const e = env();
  const f = azure();
  const now = new Date("2026-09-29T12:00:00Z");
  const ts = now.getTime() / 1000;
  const raw = JSON.stringify({ to: "pt", lines: ["a"] });
  const ok = await handle(signedReq(raw, { "x-verso-ts": String(ts), "x-verso-sig": await sign("segredo", ts, raw) }), e, {
    fetch: f,
    now,
  });
  assert.equal(ok.status, 200);

  const cases = [
    {}, // sem assinatura
    { "x-verso-ts": String(ts), "x-verso-sig": await sign("outra", ts, raw) }, // chave errada
    { "x-verso-ts": String(ts), "x-verso-sig": "zz" }, // hex inválido
    { "x-verso-ts": String(ts - 301), "x-verso-sig": await sign("segredo", ts - 301, raw) }, // velho demais
    { "x-verso-ts": String(ts + 1), "x-verso-sig": await sign("segredo", ts, raw) }, // ts adulterado
  ];
  for (const h of cases) {
    const r = await handle(signedReq(raw, h), e, { fetch: f, now });
    assert.equal(r.status, 401, JSON.stringify(h));
  }
  // Corpo adulterado com assinatura do original.
  const other = JSON.stringify({ to: "pt", lines: ["b"] });
  const r = await handle(signedReq(other, { "x-verso-ts": String(ts), "x-verso-sig": await sign("segredo", ts, raw) }), e, {
    fetch: f,
    now,
  });
  assert.equal(r.status, 401);
  assert.equal(f.calls.length, 1);
});

test("sem SIGNING_KEY no Worker recusa tudo", async () => {
  const r = await handle(await req({ to: "pt", lines: ["a"] }), env({ SIGNING_KEY: "" }), { fetch: azure() });
  assert.equal(r.status, 401);
});
