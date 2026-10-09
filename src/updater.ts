// Mises à jour de l'application depuis les releases GitHub (signées avec la clé du projet).
import { markRaw, reactive } from "vue";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** Mises à jour publiées pour Windows seulement (la version Mac est construite en local). */
export const updatesSupported = !navigator.userAgent.includes("Mac");

export const updater = reactive({
  /** Mise à jour disponible (null : à jour ou pas encore vérifié). */
  available: null as Update | null,
  checking: false,
  installing: false,
  /** Progression du téléchargement (0–100), null si inconnue. */
  progress: null as number | null,
  /** Dernière vérification sans mise à jour trouvée. */
  upToDate: false,
  error: "",
});

/** Cherche une nouvelle version ; `silent` : pas d'erreur affichée (vérification au démarrage). */
export async function checkForUpdate(silent = false) {
  if (updater.checking || updater.installing) return;
  updater.checking = true;
  updater.error = "";
  updater.upToDate = false;
  try {
    // markRaw : l'objet Update a des champs privés (#rid) qu'un Proxy réactif de Vue
    // rend illisibles (« Cannot read private member… » au téléchargement).
    const update = await check();
    updater.available = update ? markRaw(update) : null;
    updater.upToDate = !update;
  } catch (e) {
    if (!silent) updater.error = e instanceof Error ? e.message : String(e);
    else console.warn("[updater] vérification impossible :", e);
  } finally {
    updater.checking = false;
  }
}

/** Télécharge et installe la mise à jour, puis relance l'application. */
export async function installUpdate() {
  const update = updater.available;
  if (!update) return;
  updater.installing = true;
  updater.progress = 0;
  updater.error = "";
  let total = 0;
  let received = 0;
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      else if (event.event === "Progress") {
        received += event.data.chunkLength;
        updater.progress = total ? Math.min(100, Math.round((100 * received) / total)) : null;
      }
    });
    await relaunch();
  } catch (e) {
    updater.error = e instanceof Error ? e.message : String(e);
    updater.installing = false;
  }
}
