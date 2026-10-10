// Boîtes de dialogue « Ouvrir » / « Enregistrer sous » et ouverture de fichiers, passées par Rust
// (src-tauri/src/files.rs) : seuls les chemins choisis ici sont ensuite lisibles ou modifiables
// par les commandes de l'appli.
import { invoke } from "@tauri-apps/api/core";

export interface DialogFilter {
  name: string;
  extensions: string[];
}

interface PickOptions {
  title?: string;
  /** Dossier de départ ; pour « Enregistrer sous », nom (ou chemin) proposé. */
  defaultPath?: string;
  filters?: DialogFilter[];
}

const pick = (options: PickOptions & { directory?: boolean; multiple?: boolean }) =>
  invoke<string[]>("pick_open", { options });

/** Un fichier ; null si annulé. */
export const pickFile = async (options: PickOptions = {}) => (await pick(options))[0] ?? null;

/** Plusieurs fichiers ; vide si annulé. */
export const pickFiles = (options: PickOptions = {}) => pick({ ...options, multiple: true });

/** Un dossier ; null si annulé. */
export const pickFolder = async (options: PickOptions = {}) => (await pick({ ...options, directory: true }))[0] ?? null;

/** « Enregistrer sous » ; null si annulé. */
export const pickSavePath = (options: PickOptions = {}) => invoke<string | null>("pick_save", { options });

/** Ouvre avec l'application par défaut un fichier que l'appli vient d'écrire (ou le dossier des sauvegardes). */
export const openFile = (path: string) => invoke<void>("open_file", { path });
