import assert from "node:assert/strict";
import { test } from "node:test";
import { createLicenceService } from "../src/licence.js";
import { julie, MACHINE, memoryStore, OTHER_MACHINE, testSigner } from "./helpers.js";

const setup = (users = [julie()]) => {
  const store = memoryStore(users);
  const signer = testSigner();
  let t = Date.parse("2026-10-12T08:00:00Z");
  const service = createLicenceService({ store, signer, licenceDays: 15, defaultMaxDevices: 2, now: () => t });
  return { store, signer, service, advance: (days) => (t += days * 86400_000), now: () => t };
};
const login = (extra = {}) => ({ email: "julie@example.fr", password: "secret-julie", machine_id: MACHINE, machine_name: "PC-JULIE", app_version: "0.2.0", ...extra });

test("connexion : enregistre le poste et renvoie un ticket de 15 jours", async () => {
  const { store, signer, service, now } = setup();
  const r = await service.activate(login({ email: " Julie@Example.fr " }));
  const payload = signer.verify(r.ticket, now());
  assert.equal(payload.sub, "user00000000001");
  assert.equal(payload.machine, MACHINE);
  assert.equal(payload.device, r.device_id);
  assert.equal(payload.exp - payload.iat, 15 * 86400);
  const device = store.devices.get(r.device_id);
  assert.equal(device.machine_name, "PC-JULIE");
  assert.notEqual(device.secret_hash, r.secret, "seule l'empreinte du secret est stockée");
});

test("connexion refusée : mauvais mot de passe, compte non activé, demande incomplète", async () => {
  const { store, service } = setup([julie(), { ...julie(), id: "user00000000002", email: "paul@example.fr", active: false }]);
  await assert.rejects(service.activate(login({ password: "faux" })), { status: 401 });
  await assert.rejects(service.activate(login({ email: "paul@example.fr" })), { status: 403 });
  await assert.rejects(service.activate(login({ machine_id: "pas-une-empreinte" })), { status: 400 });
  await assert.rejects(service.activate({}), { status: 400 });
  assert.equal(store.devices.size, 0);
});

test("nombre de postes limité ; le même poste se reconnecte sans en prendre un de plus", async () => {
  const { store, service } = setup([{ ...julie(), max_devices: 1 }]);
  const first = await service.activate(login());
  const again = await service.activate(login());
  assert.equal(again.device_id, first.device_id);
  assert.equal(store.devices.size, 1);
  await assert.rejects(service.activate(login({ machine_id: OTHER_MACHINE })), { status: 403, message: /Nombre de postes atteint/ });
  // Déconnexion du premier poste : la place se libère.
  await service.deactivate({ device_id: again.device_id, secret: again.secret });
  await service.activate(login({ machine_id: OTHER_MACHINE }));
});

test("renouvellement : nouveau ticket avec le secret du poste, sans mot de passe", async () => {
  const { store, signer, service, advance, now } = setup();
  const { device_id, secret } = await service.activate(login());
  advance(10);
  const { ticket } = await service.renew({ device_id, secret, app_version: "0.2.1" });
  assert.ok(signer.verify(ticket, now() + 14 * 86400_000));
  assert.equal(store.devices.get(device_id).app_version, "0.2.1");
  // L'ancien secret reste valable ; une reconnexion le remplace.
  await service.activate(login());
  await assert.rejects(service.renew({ device_id, secret }), { status: 401 });
});

test("renouvellement refusé : compte désactivé, poste révoqué ou supprimé, secret faux", async () => {
  const { store, service } = setup();
  const { device_id, secret } = await service.activate(login());
  await assert.rejects(service.renew({ device_id, secret: "0".repeat(64) }), { status: 401 });
  await assert.rejects(service.renew({ device_id, secret: "court" }), { status: 400 });

  store.users[0].active = false;
  await assert.rejects(service.renew({ device_id, secret }), { status: 403 });
  store.users[0].active = true;

  store.devices.get(device_id).revoked = true;
  await assert.rejects(service.renew({ device_id, secret }), { status: 403 });
  await assert.rejects(service.activate(login()), { status: 403, message: /désactivé/ });
  // Un poste révoqué ne disparaît pas à la déconnexion (il resterait sinon réactivable).
  await service.deactivate({ device_id, secret });
  assert.ok(store.devices.has(device_id));

  store.devices.delete(device_id);
  await assert.rejects(service.renew({ device_id, secret }), { status: 401 });
});
