/**
 * Accès à PocketBase par son API REST, avec le compte superutilisateur du service.
 *
 * Tables :
 * - `users` (comptes, table d'authentification de PocketBase) : champs ajoutés `active` (accès
 *   autorisé) et `max_devices` (nombre de postes, vide = valeur par défaut du service) ;
 * - `devices` (postes) : `user`, `machine_id`, `machine_name`, `app_version`, `last_seen`,
 *   `revoked`, `secret_hash` (empreinte du secret de renouvellement du poste).
 * Toutes les règles d'accès sont fermées : seul le superutilisateur (le service, l'interface
 * d'administration) lit et écrit ; un compte ne peut ni s'inscrire ni se modifier lui-même.
 */
export class PocketBaseError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

/** Identifiant PocketBase (15 caractères a-z0-9), ou empreinte hexadécimale : sans danger dans un filtre. */
const SAFE_ID = /^[a-z0-9]{1,64}$/;

/**
 * Connexion : jeton fixe (`apiToken`, créé par « Impersonate » dans l'interface PocketBase : les
 * superutilisateurs étant soumis au MFA, c'est la façon de laisser un service se connecter seul),
 * ou email + mot de passe (sans MFA, ex. PocketBase de test).
 */
export function createPocketBase({ url, email, password, apiToken, fetch = globalThis.fetch }) {
  let token = apiToken ?? null;

  async function call(method, path, body, { auth = true, retry = true } = {}) {
    if (auth && !token) await login();
    const res = await fetch(url + path, {
      method,
      headers: {
        ...(body ? { "content-type": "application/json" } : {}),
        ...(auth ? { authorization: token } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (auth && res.status === 401 && apiToken) {
      throw new PocketBaseError(401, "jeton PB_ADMIN_TOKEN refusé (expiré, ou mot de passe du compte changé) : en créer un nouveau");
    }
    // Jeton expiré : une nouvelle connexion, une seule fois.
    if (auth && res.status === 401 && retry) {
      token = null;
      return call(method, path, body, { auth, retry: false });
    }
    if (res.status === 204) return null;
    const data = await res.json().catch(() => null);
    if (!res.ok) {
      // Mot de passe correct mais code par email exigé : le compte du service doit être exclu du MFA.
      if (data?.mfaId) {
        throw new PocketBaseError(res.status, "le compte du service est soumis à la double authentification (MFA) : utiliser un jeton PB_ADMIN_TOKEN (Impersonate)");
      }
      throw new PocketBaseError(res.status, data?.message || `PocketBase : erreur ${res.status}`);
    }
    return data;
  }

  async function login() {
    const data = await call(
      "POST",
      "/api/collections/_superusers/auth-with-password",
      { identity: email, password },
      { auth: false },
    );
    token = data.token;
  }

  const records = (collection) => `/api/collections/${collection}/records`;

  return {
    /** Compte dont l'email et le mot de passe sont corrects, sinon null. */
    async authUser(identity, pass) {
      try {
        const data = await call("POST", "/api/collections/users/auth-with-password", { identity, password: pass }, { auth: false });
        return data.record;
      } catch (e) {
        if (e instanceof PocketBaseError && e.status === 400) return null;
        throw e;
      }
    },

    async getUser(id) {
      if (!SAFE_ID.test(id)) return null;
      try {
        return await call("GET", `${records("users")}/${id}`);
      } catch (e) {
        if (e instanceof PocketBaseError && e.status === 404) return null;
        throw e;
      }
    },

    async getDevice(id) {
      if (!SAFE_ID.test(id)) return null;
      try {
        return await call("GET", `${records("devices")}/${id}`);
      } catch (e) {
        if (e instanceof PocketBaseError && e.status === 404) return null;
        throw e;
      }
    },

    /** Postes du compte. */
    async listDevices(userId) {
      if (!SAFE_ID.test(userId)) return [];
      const filter = encodeURIComponent(`user = "${userId}"`);
      const data = await call("GET", `${records("devices")}?filter=${filter}&perPage=200&skipTotal=1`);
      return data.items;
    },

    createDevice(data) {
      return call("POST", records("devices"), data);
    },

    updateDevice(id, data) {
      return call("PATCH", `${records("devices")}/${id}`, data);
    },

    deleteDevice(id) {
      return call("DELETE", `${records("devices")}/${id}`);
    },

    /** Crée ou complète les tables (sans rien supprimer) : à chaque démarrage du service. */
    async ensureSchema() {
      const closed = { listRule: null, viewRule: null, createRule: null, updateRule: null, deleteRule: null };
      const users = await call("GET", "/api/collections/users");
      const extra = [
        { name: "active", type: "bool" },
        { name: "max_devices", type: "number", onlyInt: true, min: 0 },
      ].filter((f) => !users.fields.some((u) => u.name === f.name));
      await call("PATCH", "/api/collections/users", { ...closed, fields: [...users.fields, ...extra] });

      const exists = await call("GET", "/api/collections/devices").catch((e) => {
        if (e instanceof PocketBaseError && e.status === 404) return null;
        throw e;
      });
      if (exists) return;
      await call("POST", "/api/collections", {
        name: "devices",
        type: "base",
        ...closed,
        fields: [
          { name: "user", type: "relation", collectionId: users.id, required: true, maxSelect: 1, cascadeDelete: true },
          { name: "machine_id", type: "text", required: true, max: 64 },
          { name: "machine_name", type: "text", max: 100 },
          { name: "app_version", type: "text", max: 20 },
          { name: "last_seen", type: "date" },
          { name: "revoked", type: "bool" },
          { name: "secret_hash", type: "text", hidden: true, max: 64 },
          { name: "created", type: "autodate", onCreate: true },
          { name: "updated", type: "autodate", onCreate: true, onUpdate: true },
        ],
        indexes: ["CREATE UNIQUE INDEX idx_devices_user_machine ON devices (user, machine_id)"],
      });
    },
  };
}
