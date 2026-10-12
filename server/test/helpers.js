import { generateKeyPairSync } from "node:crypto";
import { createSigner } from "../src/ticket.js";

export const testSigner = () =>
  createSigner(generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" }));

export const MACHINE = "a".repeat(64);
export const OTHER_MACHINE = "b".repeat(64);

/** PocketBase en mémoire (mêmes méthodes que createPocketBase). */
export function memoryStore(users = []) {
  const devices = new Map();
  let next = 0;
  const id = () => `dev${String(++next).padStart(12, "0")}`;
  return {
    users,
    devices,
    async authUser(email, password) {
      const u = users.find((u) => u.email === email && u.password === password);
      return u ? { ...u } : null;
    },
    async getUser(uid) {
      return users.find((u) => u.id === uid) ?? null;
    },
    async getDevice(did) {
      return devices.has(did) ? { ...devices.get(did) } : null;
    },
    async listDevices(uid) {
      return [...devices.values()].filter((d) => d.user === uid).map((d) => ({ ...d }));
    },
    async createDevice(data) {
      const d = { id: id(), ...data };
      devices.set(d.id, d);
      return { ...d };
    },
    async updateDevice(did, data) {
      Object.assign(devices.get(did), data);
      return { ...devices.get(did) };
    },
    async deleteDevice(did) {
      devices.delete(did);
    },
  };
}

export const julie = () => ({ id: "user00000000001", email: "julie@example.fr", password: "secret-julie", name: "Julie", active: true, max_devices: 0 });
