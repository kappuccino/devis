import assert from "node:assert/strict";
import { test } from "node:test";
import { createLicenceService } from "../src/licence.js";
import { createPocketBase } from "../src/pocketbase.js";
import { MACHINE, testSigner } from "./helpers.js";

// Test d'intégration avec un vrai PocketBase (vide), lancé à part :
//   PB_TEST_URL=http://127.0.0.1:8099 PB_TEST_EMAIL=… PB_TEST_PASSWORD=… npm test
const { PB_TEST_URL: url, PB_TEST_EMAIL: email, PB_TEST_PASSWORD: password } = process.env;

test("PocketBase : tables créées, connexion, postes", { skip: !url && "PB_TEST_URL non défini" }, async () => {
  const pb = createPocketBase({ url, email, password });
  await pb.ensureSchema();
  await pb.ensureSchema(); // sans effet la deuxième fois

  // Compte créé comme depuis l'interface d'administration.
  const admin = await fetch(`${url}/api/collections/_superusers/auth-with-password`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ identity: email, password }),
  }).then((r) => r.json());
  const userEmail = `julie-${Date.now()}@example.fr`;
  const res = await fetch(`${url}/api/collections/users/records`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: admin.token },
    body: JSON.stringify({ email: userEmail, password: "secret-julie", passwordConfirm: "secret-julie", name: "Julie", active: true }),
  });
  assert.equal(res.status, 200, await res.clone().text());
  const created = await res.json();

  // Règles fermées : ni inscription, ni lecture sans être superutilisateur.
  const signup = await fetch(`${url}/api/collections/users/records`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ email: "pirate@example.fr", password: "12345678", passwordConfirm: "12345678", active: true }),
  });
  assert.equal(signup.status, 403);
  assert.equal((await fetch(`${url}/api/collections/devices/records`)).status, 403);

  const service = createLicenceService({ store: pb, signer: testSigner(), licenceDays: 15, defaultMaxDevices: 2 });
  await assert.rejects(service.activate({ email: userEmail, password: "faux", machine_id: MACHINE }), { status: 401 });
  const { device_id, secret, ticket } = await service.activate({ email: userEmail, password: "secret-julie", machine_id: MACHINE, machine_name: "PC" });
  assert.ok(ticket);
  const [device] = await pb.listDevices(created.id);
  assert.equal(device.id, device_id);
  assert.equal(device.machine_name, "PC");
  await service.renew({ device_id, secret });
  await service.deactivate({ device_id, secret });
  assert.deepEqual(await pb.listDevices(created.id), []);
});
