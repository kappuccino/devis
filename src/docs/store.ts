// État partagé de la documentation : réglages, index, indexation en tâche de fond.
import { computed, reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../api";
import { getStats } from "./core/index.js";
import { openIndex, type IndexInfo } from "./db";
import { indexFolder, type IndexProgress, type IndexSummary } from "./indexer";

/** Réglages (table settings de Devis). */
const FOLDER_KEY = "docs_folder";
const STARTUP_KEY = "docs_index_on_startup";

export const docs = reactive({
  info: null as IndexInfo | null,
  stats: { docs: 0, images: 0, pages: 0 },
  /** Dossier indexé (sous-dossiers compris). */
  folder: null as string | null,
  indexOnStartup: true,
  progress: null as IndexProgress | null,
  /** Message de fin d'indexation. */
  notice: "",
  /** Fichiers non indexés lors de la dernière passe. */
  errors: [] as IndexSummary["errors"],
  /** Incrémenté à chaque fin d'indexation : les recherches affichées sont relancées. */
  version: 0,
  indexPath: "",
});

export const indexing = computed(() => docs.progress !== null);

async function refreshStats() {
  const { db } = await openIndex();
  docs.stats = await getStats(db);
}

let started: Promise<void> | undefined;

/**
 * Au démarrage de l'appli : ouvre l'index, charge les réglages (au premier lancement, reprend le
 * dossier de l'ancienne app PDF Finder s'il existe) et lance l'indexation en tâche de fond.
 */
export function startDocs() {
  started ??= (async () => {
    try {
      const { info } = await openIndex();
      docs.info = info;
      docs.indexPath = await invoke<string>("docs_index_path");
      const settings = await api.getSettings();
      docs.folder = settings[FOLDER_KEY] || null;
      docs.indexOnStartup = settings[STARTUP_KEY] !== "0";
      if (!docs.folder && !(FOLDER_KEY in settings)) {
        const legacy = await invoke<string | null>("docs_legacy_folder");
        if (legacy) {
          docs.folder = legacy;
          await api.saveSettings({ [FOLDER_KEY]: legacy });
        }
      }
      await refreshStats();
    } catch (e) {
      docs.notice = `Documentation indisponible : ${e instanceof Error ? e.message : String(e)}`;
      return;
    }
    // Sans attendre : l'appli reste utilisable pendant l'indexation.
    if (docs.folder && docs.indexOnStartup) reindex();
  })();
  return started;
}

/** Met l'index à jour (incrémental : seuls les fichiers nouveaux ou modifiés sont relus). */
export async function reindex() {
  if (!docs.folder || indexing.value) return;
  const { db } = await openIndex();
  docs.notice = "";
  docs.progress = { index: 0, total: 0, file: "" };
  try {
    const s = await indexFolder(db, docs.folder, (p) => (docs.progress = p));
    docs.errors = s.errors;
    docs.notice =
      `Index à jour : ${s.files - s.images} PDF et ${s.images} images ` +
      `(${s.added} ajoutés, ${s.updated} modifiés, ${s.removed} retirés)` +
      ` · ${s.pages} pages lues en ${s.seconds.toFixed(1)} s` +
      (s.reused ? ` · ${s.reused} doublons repris sans relecture` : "");
  } catch (e) {
    docs.errors = [{ file: docs.folder, message: e instanceof Error ? e.message : String(e) }];
    docs.notice = "Indexation impossible";
  } finally {
    docs.progress = null;
    await refreshStats();
    docs.version++;
  }
}

export async function setFolder(dir: string) {
  docs.folder = dir;
  await api.saveSettings({ [FOLDER_KEY]: dir });
  await reindex();
}

export async function setIndexOnStartup(value: boolean) {
  docs.indexOnStartup = value;
  await api.saveSettings({ [STARTUP_KEY]: value ? "1" : "0" });
}
