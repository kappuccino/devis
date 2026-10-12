// Crée la paire de clés des tickets de licence.
//   node scripts/keygen.js <fichier de la clé privée>
// La clé privée est écrite dans le fichier (droits 600, jamais affichée) ; la clé publique,
// à embarquer dans l'appli, est affichée.
import { generateKeyPairSync } from "node:crypto";
import { writeFileSync } from "node:fs";
import { createSigner } from "../src/ticket.js";

const target = process.argv[2];
if (!target) {
  console.error("Usage : node scripts/keygen.js <fichier de la clé privée>");
  process.exit(1);
}
const { privateKey } = generateKeyPairSync("ed25519");
const pem = privateKey.export({ type: "pkcs8", format: "pem" });
// « wx » : ne remplace jamais une clé existante (les postes ne pourraient plus vérifier leurs tickets).
writeFileSync(target, pem, { mode: 0o600, flag: "wx" });
console.log(`Clé privée écrite dans ${target}`);
console.log(`Clé publique (pour l'appli) : ${createSigner(pem).publicKey}`);
