import assert from "node:assert/strict";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { createApp } from "../src/app.js";
import { createLicenceService } from "../src/licence.js";
import { createReleases } from "../src/releases.js";
import { julie, MACHINE, memoryStore, testSigner } from "./helpers.js";

const TOKEN = "jeton-ci-de-test";
let server, base, dir;

before(async () => {
  dir = await mkdtemp(join(tmpdir(), "licence-"));
  const signer = testSigner();
  const licence = createLicenceService({ store: memoryStore([julie()]), signer, licenceDays: 15, defaultMaxDevices: 2 });
  const app = createApp({ licence, releases: createReleases(dir), signer, releaseToken: TOKEN, trustProxy: false });
  server = app.listen(0);
  await new Promise((r) => server.once("listening", r));
  base = `http://127.0.0.1:${server.address().port}`;
});
after(async () => {
  server.close();
  await rm(dir, { recursive: true, force: true });
});

const post = (path, body) =>
  fetch(base + path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
const put = (path, body, token = TOKEN) => fetch(base + path, { method: "PUT", headers: { authorization: `Bearer ${token}` }, body });
const get = (path, ticket) => fetch(base + path, { headers: ticket ? { authorization: `Bearer ${ticket}` } : {} });
const activate = async () =>
  (await post("/api/licence/activate", { email: "julie@example.fr", password: "secret-julie", machine_id: MACHINE })).json();

const manifest = {
  version: "0.2.0",
  notes: "Version 0.2.0",
  pub_date: "2026-10-12T08:00:00Z",
  platforms: { "windows-x86_64": { signature: "sig", url: "https://licence.example/updates/0.2.0/Devis_0.2.0_x64-setup.exe" } },
};

test("santé et clé publique", async () => {
  assert.deepEqual(await (await get("/health")).json(), { ok: true });
  const { key } = await (await get("/api/public-key")).json();
  assert.equal(Buffer.from(key, "base64").length, 32);
});

test("connexion : ticket, ou erreur en français", async () => {
  const ok = await activate();
  assert.ok(ok.ticket && ok.device_id && ok.secret);
  const bad = await post("/api/licence/activate", { email: "julie@example.fr", password: "faux", machine_id: MACHINE });
  assert.equal(bad.status, 401);
  assert.deepEqual(await bad.json(), { error: "Email ou mot de passe incorrect." });
  const broken = await fetch(base + "/api/licence/renew", { method: "POST", headers: { "content-type": "application/json" }, body: "{" });
  assert.equal(broken.status, 400);
  const huge = await post("/api/licence/renew", { secret: "x".repeat(10_000) });
  assert.equal(huge.status, 413);
});

test("mises à jour : dépôt par la CI, téléchargement réservé aux postes munis d'un ticket", async () => {
  const { ticket } = await activate();
  assert.equal((await get("/updates/latest.json", ticket)).status, 204, "aucune version déposée");

  assert.equal((await put("/api/releases/0.2.0/setup.exe", "binaire", "mauvais-jeton")).status, 401);
  assert.equal((await put("/api/releases/0.2.0/..%2Fsetup.exe", "binaire")).status, 400);
  assert.equal((await put("/api/releases/latest.json", "{pas du json")).status, 400);
  assert.equal((await put("/api/releases/0.2.0/Devis_0.2.0_x64-setup.exe", "binaire")).status, 200);
  assert.equal((await put("/api/releases/latest.json", JSON.stringify(manifest))).status, 200);
  assert.deepEqual(await readdir(join(dir, "releases", "0.2.0")), ["Devis_0.2.0_x64-setup.exe"]);

  assert.equal((await get("/updates/latest.json")).status, 401);
  assert.equal((await get("/updates/latest.json", "faux.ticket")).status, 401);
  assert.deepEqual(await (await get("/updates/latest.json", ticket)).json(), manifest);
  const file = await get("/updates/0.2.0/Devis_0.2.0_x64-setup.exe", ticket);
  assert.equal(file.status, 200);
  assert.equal(await file.text(), "binaire");
  assert.equal((await get("/updates/0.2.0/Devis_0.2.0_x64-setup.exe")).status, 401);
  assert.equal((await get("/updates/0.2.0/absent.exe", ticket)).status, 404);
});

test("PocketBase pas encore prêt : 503 sur /health et les licences", async () => {
  const signer = testSigner();
  const licence = createLicenceService({ store: memoryStore([julie()]), signer, licenceDays: 15, defaultMaxDevices: 2 });
  const app = createApp({ licence, releases: createReleases(dir), signer, releaseToken: TOKEN, trustProxy: false, status: () => ({ ready: false, error: "MFA" }) });
  const s = app.listen(0);
  await new Promise((r) => s.once("listening", r));
  const url = `http://127.0.0.1:${s.address().port}`;
  try {
    const health = await fetch(url + "/health");
    assert.equal(health.status, 503);
    assert.deepEqual(await health.json(), { ok: false }, "le motif reste dans les journaux");
    const res = await fetch(url + "/api/licence/activate", { method: "POST", headers: { "content-type": "application/json" }, body: "{}" });
    assert.equal(res.status, 503);
  } finally {
    s.close();
  }
});

test("trop de tentatives de connexion : 429", async () => {
  let last;
  for (let i = 0; i < 12; i++) last = await post("/api/licence/activate", { email: "x@y.fr", password: "z", machine_id: MACHINE });
  assert.equal(last.status, 429);
});
