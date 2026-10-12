import { createApp } from "./app.js";
import { loadConfig } from "./config.js";
import { createLicenceService } from "./licence.js";
import { createPocketBase } from "./pocketbase.js";
import { createReleases } from "./releases.js";
import { createSigner } from "./ticket.js";

const config = loadConfig();
const signer = createSigner(config.privateKey);
const store = createPocketBase({ url: config.pbUrl, email: config.pbEmail, password: config.pbPassword });

// PocketBase peut démarrer après le service : on réessaie pendant 2 minutes.
for (let attempt = 1; ; attempt++) {
  try {
    await store.ensureSchema();
    break;
  } catch (e) {
    if (attempt >= 24) throw e;
    console.warn(`PocketBase injoignable (${e.message}), nouvel essai dans 5 s…`);
    await new Promise((r) => setTimeout(r, 5000));
  }
}

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
});

const server = app.listen(config.port, () => {
  console.log(`Service de licences sur le port ${config.port} (clé publique : ${signer.publicKey})`);
});
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => server.close(() => process.exit(0)));
