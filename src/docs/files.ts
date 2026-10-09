// Accès aux fichiers de la documentation (commandes Rust).
import { invoke } from "@tauri-apps/api/core";

export interface DocFile {
  path: string;
  /** Date de création (ms), ou de modification si le système ne la fournit pas. */
  createdAt: number | null;
}

/** PDF et images (jpg/png) du dossier, sous-dossiers compris ; fichiers cachés ignorés. */
export const listFiles = (dir: string) => invoke<DocFile[]>("docs_list_files", { dir });

/** Contenu brut d'un fichier. */
export async function readFile(path: string): Promise<Uint8Array> {
  return new Uint8Array(await invoke<ArrayBuffer>("read_file", { path }));
}

export const copyFile = (from: string, to: string) => invoke<void>("copy_file", { from, to });

/** Fichier temporaire pour le glisser-déposer ; renvoie son chemin. */
export const writeDragFile = (name: string, contents: Uint8Array) =>
  invoke<string>("write_drag_file", { name, contents: Array.from(contents) });

export const isMac = navigator.userAgent.includes("Mac");
export const fileManager = isMac ? "le Finder" : "l’Explorateur";

export const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function dirName(p: string) {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i > 0 ? p.slice(0, i) : p;
}

const dateFormat = new Intl.DateTimeFormat("fr-FR", { day: "2-digit", month: "2-digit", year: "numeric" });

/** Date de création affichable (ms → jj/mm/aaaa). */
export const formatFileDate = (ms: number | null) => (ms ? dateFormat.format(new Date(ms)) : "date inconnue");
