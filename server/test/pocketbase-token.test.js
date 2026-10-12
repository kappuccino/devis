import assert from "node:assert/strict";
import { test } from "node:test";
import { createPocketBase } from "../src/pocketbase.js";

/** fetch simulé : enregistre les appels, répond `status`. */
const fakeFetch = (status) => {
  const calls = [];
  const fetch = async (url, init) => {
    calls.push({ url, auth: init.headers.authorization });
    return new Response(status === 200 ? JSON.stringify({ id: "user00000000001" }) : "{}", { status });
  };
  return { calls, fetch };
};

test("jeton fixe (PB_ADMIN_TOKEN) : utilisé tel quel, sans connexion par mot de passe", async () => {
  const { calls, fetch } = fakeFetch(200);
  const pb = createPocketBase({ url: "https://pb.test", apiToken: "jeton-fixe", fetch });
  assert.equal((await pb.getUser("user00000000001")).id, "user00000000001");
  assert.deepEqual(calls, [{ url: "https://pb.test/api/collections/users/records/user00000000001", auth: "jeton-fixe" }]);
});

test("jeton fixe refusé : message clair, pas de tentative de connexion", async () => {
  const { calls, fetch } = fakeFetch(401);
  const pb = createPocketBase({ url: "https://pb.test", apiToken: "jeton-expire", fetch });
  await assert.rejects(pb.getUser("user00000000001"), { status: 401, message: /PB_ADMIN_TOKEN/ });
  assert.equal(calls.length, 1);
});
