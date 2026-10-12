import { createHash, randomBytes, timingSafeEqual } from "node:crypto";

/** Erreur renvoyée telle quelle à l'appli (message affiché à l'utilisateur). */
export class HttpError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

const sha256 = (s) => createHash("sha256").update(s).digest("hex");
/** Identifiant de la machine : empreinte SHA-256 calculée par l'appli (jamais l'identifiant brut). */
const MACHINE_ID = /^[a-f0-9]{64}$/;
const SECRET = /^[a-f0-9]{64}$/;
const DEVICE_ID = /^[a-z0-9]{15}$/;

const text = (v, max) => (typeof v === "string" ? v.trim().slice(0, max) : "");

/**
 * Comptes et postes :
 * - activate : email + mot de passe → enregistre le poste (dans la limite du compte), renvoie un
 *   ticket et un secret propre au poste ;
 * - renew : secret du poste → nouveau ticket (sans mot de passe), tant que le compte est actif
 *   et le poste non révoqué ;
 * - deactivate : libère le poste (« Se déconnecter » dans l'appli).
 */
export function createLicenceService({ store, signer, licenceDays, defaultMaxDevices, now = () => Date.now() }) {
  const ticketFor = (user, device) => {
    const iat = Math.floor(now() / 1000);
    return signer.sign({
      v: 1,
      sub: user.id,
      email: user.email,
      name: user.name ?? "",
      device: device.id,
      machine: device.machine_id,
      iat,
      exp: iat + licenceDays * 86400,
    });
  };

  const checkUser = (user) => {
    if (!user?.active) throw new HttpError(403, "Ce compte n'est pas activé. Contactez l'administrateur de l'appli.");
  };

  /** Poste du compte dont le secret correspond, sinon erreur. */
  async function deviceWithSecret(body) {
    const deviceId = text(body?.device_id, 20);
    const secret = text(body?.secret, 64);
    if (!DEVICE_ID.test(deviceId) || !SECRET.test(secret)) throw new HttpError(400, "Demande invalide.");
    const device = await store.getDevice(deviceId);
    const expected = Buffer.from(device?.secret_hash ?? "", "utf8");
    const given = Buffer.from(sha256(secret), "utf8");
    if (!device || expected.length !== given.length || !timingSafeEqual(expected, given)) {
      throw new HttpError(401, "Ce poste n'est plus enregistré. Reconnectez-vous.");
    }
    return device;
  }

  return {
    async activate(body) {
      const email = text(body?.email, 200).toLowerCase();
      const password = typeof body?.password === "string" ? body.password : "";
      const machineId = text(body?.machine_id, 64);
      if (!email || !password || password.length > 200 || !MACHINE_ID.test(machineId)) {
        throw new HttpError(400, "Email, mot de passe et poste requis.");
      }
      const user = await store.authUser(email, password);
      if (!user) throw new HttpError(401, "Email ou mot de passe incorrect.");
      checkUser(user);

      const devices = await store.listDevices(user.id);
      let device = devices.find((d) => d.machine_id === machineId);
      if (device?.revoked) throw new HttpError(403, "Ce poste a été désactivé. Contactez l'administrateur de l'appli.");
      const secret = randomBytes(32).toString("hex");
      const fields = {
        machine_name: text(body?.machine_name, 100),
        app_version: text(body?.app_version, 20),
        last_seen: new Date(now()).toISOString(),
        secret_hash: sha256(secret),
      };
      if (device) {
        device = await store.updateDevice(device.id, fields);
      } else {
        const max = user.max_devices || defaultMaxDevices;
        if (devices.filter((d) => !d.revoked).length >= max) {
          throw new HttpError(
            403,
            `Nombre de postes atteint pour ce compte (${max}). Déconnectez un autre poste ou contactez l'administrateur.`,
          );
        }
        device = await store.createDevice({ user: user.id, machine_id: machineId, revoked: false, ...fields });
      }
      return { ticket: ticketFor(user, device), device_id: device.id, secret };
    },

    async renew(body) {
      const device = await deviceWithSecret(body);
      if (device.revoked) throw new HttpError(403, "Ce poste a été désactivé. Contactez l'administrateur de l'appli.");
      const user = await store.getUser(device.user);
      checkUser(user);
      const updated = await store.updateDevice(device.id, {
        last_seen: new Date(now()).toISOString(),
        app_version: text(body?.app_version, 20) || device.app_version,
      });
      return { ticket: ticketFor(user, updated) };
    },

    async deactivate(body) {
      const device = await deviceWithSecret(body);
      // Un poste révoqué le reste (sinon une reconnexion le réactiverait).
      if (!device.revoked) await store.deleteDevice(device.id);
      return { ok: true };
    },
  };
}
