import { createApp } from "./app.js";
import { loadConfig } from "./config.js";
import { createLicenceService } from "./licence.js";
import { createPocketBase } from "./pocketbase.js";
import { createReleases } from "./releases.js";
import { createSigner } from "./ticket.js";

const config = loadConfig();
const signer = createSigner(config.privateKey);
const store = createPocketBase({ url: config.pbUrl, email: config.pbEmail, password: config.pbPassword, apiToken: config.pbToken });

// Le service écoute tout de suite ; les tables PocketBase sont créées dès que PocketBase répond
// (nouvel essai toutes les 10 s, motif dans les journaux et sur /health).
const state = { ready: false, error: null };
(async function prepare() {
  for (;;) {
    try {
      await store.ensureSchema();
      Object.assign(state, { ready: true, error: null });
      console.log("PocketBase prêt (tables à jour).");
      return;
    } catch (e) {
      if (e.message !== state.error) console.warn(`PocketBase indisponible : ${e.message}. Nouvel essai toutes les 10 s.`);
      state.error = e.message;
      await new Promise((r) => setTimeout(r, 10_000));
    }
  }
})();

const app = createApp({
  licence: createLicenceService({
    store,
    signer,
    licenceDays: config.licenceDays,
    defaultMaxDevices: config.defaultMaxDevices,
  }),
  releases: createReleases(config.dataDir),
  signer,
  releaseToken: config.releaseToken,
  status: () => state,
});

const server = app.listen(config.port, () => {
  console.log(`Service de licences sur le port ${config.port} (clé publique : ${signer.publicKey})`);
});
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => server.close(() => process.exit(0)));
