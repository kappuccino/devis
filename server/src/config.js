import { readFileSync } from "node:fs";

/**
 * Configuration lue dans l'environnement (variables définies dans Coolify).
 * Les secrets (mot de passe PocketBase, clé privée, jeton de dépôt) n'ont pas de valeur par défaut.
 */
export function loadConfig(env = process.env) {
  const required = (name) => {
    const v = env[name]?.trim();
    if (!v) throw new Error(`Variable d'environnement manquante : ${name}`);
    return v;
  };
  const int = (name, def) => {
    const v = env[name]?.trim();
    if (!v) return def;
    const n = Number(v);
    if (!Number.isInteger(n) || n <= 0) throw new Error(`${name} doit être un entier positif`);
    return n;
  };
  // Clé privée Ed25519 (PEM) : dans un fichier (secret monté) ou directement dans la variable.
  const privateKey = env.LICENCE_PRIVATE_KEY_FILE
    ? readFileSync(env.LICENCE_PRIVATE_KEY_FILE, "utf8")
    : required("LICENCE_PRIVATE_KEY").replace(/\\n/g, "\n");

  return {
    port: int("PORT", 3000),
    pbUrl: required("PB_URL").replace(/\/+$/, ""),
    // Jeton fixe du superutilisateur du service, sinon email + mot de passe.
    pbToken: env.PB_ADMIN_TOKEN?.trim() || null,
    pbEmail: env.PB_ADMIN_TOKEN?.trim() ? null : required("PB_ADMIN_EMAIL"),
    pbPassword: env.PB_ADMIN_TOKEN?.trim() ? null : required("PB_ADMIN_PASSWORD"),
    privateKey,
    // Durée de validité d'un ticket : l'appli fonctionne hors ligne jusqu'à son expiration.
    licenceDays: int("LICENCE_DAYS", 15),
    // Postes par compte quand « max_devices » est vide dans PocketBase.
    defaultMaxDevices: int("DEFAULT_MAX_DEVICES", 2),
    // Installeurs et latest.json (volume Docker).
    dataDir: env.DATA_DIR?.trim() || "/data",
    // Jeton de la CI pour déposer les versions.
    releaseToken: required("RELEASE_UPLOAD_TOKEN"),
  };
}
