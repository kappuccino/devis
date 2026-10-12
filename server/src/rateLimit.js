import { HttpError } from "./licence.js";

/**
 * Limite le nombre de tentatives par clé (adresse IP) sur une fenêtre de temps : contre l'essai
 * de mots de passe en série. En mémoire : remis à zéro au redémarrage du service.
 */
export function rateLimit({ max, windowMs, now = () => Date.now() }) {
  const hits = new Map();
  return (req, _res, next) => {
    const t = now();
    const key = req.ip ?? "?";
    let entry = hits.get(key);
    if (!entry || entry.reset <= t) {
      entry = { count: 0, reset: t + windowMs };
      hits.set(key, entry);
    }
    entry.count += 1;
    // Ménage des entrées expirées, de temps en temps.
    if (hits.size > 10_000) for (const [k, e] of hits) if (e.reset <= t) hits.delete(k);
    if (entry.count > max) return next(new HttpError(429, "Trop de tentatives. Réessayez dans quelques minutes."));
    next();
  };
}
