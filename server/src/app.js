import { createHash, timingSafeEqual } from "node:crypto";
import express from "express";
import { HttpError } from "./licence.js";
import { rateLimit } from "./rateLimit.js";

const bearer = (req) => {
  const h = req.get("authorization") ?? "";
  return h.startsWith("Bearer ") ? h.slice(7).trim() : "";
};

/** Comparaison à durée constante (empreintes de même longueur). */
const sameSecret = (a, b) => {
  const h = (s) => createHash("sha256").update(s).digest();
  return timingSafeEqual(h(a), h(b)) && a.length > 0;
};

/**
 * Routes :
 * - `POST /api/licence/activate|renew|deactivate` : appli (voir licence.js) ;
 * - `GET /api/public-key` : clé publique des tickets (celle embarquée dans l'appli) ;
 * - `PUT /api/releases/...` : dépôt des versions par la CI (jeton RELEASE_UPLOAD_TOKEN) ;
 * - `GET /updates/...` : mises à jour, réservées aux postes munis d'un ticket valide ;
 * - `GET /health` : contrôle de santé (Docker, Coolify).
 */
export function createApp({ licence, releases, signer, releaseToken, trustProxy = 1, now = () => Date.now(), status = () => ({ ready: true }) }) {
  const app = express();
  app.disable("x-powered-by");
  // Derrière le proxy de Coolify : adresse IP du client pour la limite de tentatives.
  app.set("trust proxy", trustProxy);
  app.use((_req, res, next) => {
    res.set({ "x-content-type-options": "nosniff", "cache-control": "no-store", "referrer-policy": "no-referrer" });
    next();
  });

  // Tant que PocketBase n'est pas prêt (tables créées), le service répond 503 (motif dans les journaux).
  app.get("/health", (_req, res) => {
    const { ready } = status();
    res.status(ready ? 200 : 503).json({ ok: ready });
  });
  app.use("/api/licence", (_req, _res, next) =>
    next(status().ready ? undefined : new HttpError(503, "Service de licences momentanément indisponible. Réessayez plus tard.")),
  );
  app.get("/api/public-key", (_req, res) => res.json({ algorithm: "Ed25519", key: signer.publicKey }));

  const json = express.json({ limit: "4kb" });
  const route = (fn) => async (req, res, next) => {
    try {
      res.json(await fn(req.body ?? {}));
    } catch (e) {
      next(e);
    }
  };
  app.post("/api/licence/activate", rateLimit({ max: 10, windowMs: 15 * 60_000, now }), json, route(licence.activate));
  app.post("/api/licence/renew", rateLimit({ max: 60, windowMs: 15 * 60_000, now }), json, route(licence.renew));
  app.post("/api/licence/deactivate", rateLimit({ max: 30, windowMs: 15 * 60_000, now }), json, route(licence.deactivate));

  // Dépôt des versions (CI).
  const ci = (req, _res, next) =>
    next(sameSecret(bearer(req), releaseToken) ? undefined : new HttpError(401, "Jeton de dépôt invalide."));
  app.put("/api/releases/latest.json", ci, async (req, res, next) => {
    try {
      res.json(await releases.saveManifest(req));
    } catch (e) {
      next(e);
    }
  });
  app.put("/api/releases/:version/:file", ci, async (req, res, next) => {
    try {
      res.json(await releases.saveFile(req.params.version, req.params.file, req));
    } catch (e) {
      next(e);
    }
  });

  // Mises à jour : ticket de licence valide exigé.
  const licensed = (req, _res, next) =>
    next(signer.verify(bearer(req), now()) ? undefined : new HttpError(401, "Licence invalide ou expirée."));
  app.get("/updates/latest.json", licensed, async (_req, res, next) => {
    try {
      const manifest = await releases.manifest();
      if (!manifest) return res.status(204).end();
      res.type("application/json").send(manifest);
    } catch (e) {
      next(e);
    }
  });
  app.get("/updates/:version/:file", licensed, async (req, res, next) => {
    try {
      const path = await releases.existingFile(req.params.version, req.params.file);
      if (!path) throw new HttpError(404, "Fichier introuvable.");
      res.set("cache-control", "private, max-age=0");
      res.download(path, req.params.file);
    } catch (e) {
      next(e);
    }
  });

  app.use((_req, _res, next) => next(new HttpError(404, "Introuvable.")));
  // Erreurs : message en français pour l'appli ; détail des erreurs internes seulement dans les journaux.
  app.use((err, req, res, _next) => {
    const status = err instanceof HttpError ? err.status : err.type === "entity.parse.failed" ? 400 : err.status === 413 ? 413 : 500;
    if (status === 500) console.error(`${req.method} ${req.path}`, err);
    const message =
      err instanceof HttpError ? err.message : status === 400 ? "Demande invalide." : status === 413 ? "Demande trop volumineuse." : "Erreur du service de licences.";
    res.status(status).json({ error: message });
  });
  return app;
}
