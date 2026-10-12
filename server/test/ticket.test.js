import assert from "node:assert/strict";
import { test } from "node:test";
import { testSigner } from "./helpers.js";

test("un ticket signé se vérifie, jusqu'à son expiration", () => {
  const signer = testSigner();
  const exp = Math.floor(Date.now() / 1000) + 60;
  const ticket = signer.sign({ sub: "u1", exp });
  assert.deepEqual(signer.verify(ticket), { sub: "u1", exp });
  assert.equal(signer.verify(ticket, (exp + 1) * 1000), null);
  assert.equal(Buffer.from(signer.publicKey, "base64").length, 32);
});

test("un ticket modifié, signé par une autre clé ou mal formé est refusé", () => {
  const signer = testSigner();
  const exp = Math.floor(Date.now() / 1000) + 60;
  const ticket = signer.sign({ sub: "u1", exp });
  const [, sig] = ticket.split(".");
  const forged = `${Buffer.from(JSON.stringify({ sub: "u1", exp: exp + 86400 * 365 })).toString("base64url")}.${sig}`;
  assert.equal(signer.verify(forged), null);
  assert.equal(signer.verify(testSigner().sign({ sub: "u1", exp })), null);
  for (const bad of ["", "abc", "a.b", `${ticket}.x`, null, 42, "x".repeat(5000)]) assert.equal(signer.verify(bad), null);
});
