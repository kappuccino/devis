import { createPrivateKey, createPublicKey, sign, verify } from "node:crypto";

/**
 * Ticket de licence : `<données en base64url>.<signature Ed25519 en base64url>`.
 * Signé avec la clé privée du serveur ; l'appli le vérifie avec la clé publique qu'elle embarque,
 * et fonctionne hors ligne jusqu'à `exp` (secondes depuis 1970).
 */
const b64url = (buf) => Buffer.from(buf).toString("base64url");

export function createSigner(privatePem) {
  const key = createPrivateKey(privatePem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("La clé de licence doit être une clé Ed25519");
  const publicKey = createPublicKey(key);
  return {
    /** Clé publique brute (32 octets) en base64 : à embarquer dans l'appli. */
    publicKey: Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url").toString("base64"),
    sign(payload) {
      const data = b64url(JSON.stringify(payload));
      return `${data}.${b64url(sign(null, Buffer.from(data), key))}`;
    },
    /** Contenu du ticket s'il est authentique et non expiré, sinon null. */
    verify(ticket, now = Date.now()) {
      return verifyTicket(ticket, publicKey, now);
    },
  };
}

export function verifyTicket(ticket, publicKey, now = Date.now()) {
  if (typeof ticket !== "string" || ticket.length > 4096) return null;
  const [data, sig, extra] = ticket.split(".");
  if (!data || !sig || extra !== undefined) return null;
  try {
    if (!verify(null, Buffer.from(data), publicKey, Buffer.from(sig, "base64url"))) return null;
    const payload = JSON.parse(Buffer.from(data, "base64url").toString("utf8"));
    if (typeof payload.exp !== "number" || payload.exp * 1000 <= now) return null;
    return payload;
  } catch {
    return null;
  }
}
