// Glisser-déposer natif vers une autre application (Finder, mail, Teams…) :
// - résultat « page » : le PDF de la page est généré à la volée (dossier temporaire de l'appli) ;
// - résultat « fichier entier » (image, PDF trouvé par son nom) : le fichier d'origine tel quel.
import { startDrag } from "@crabnebula/tauri-plugin-drag";
import type { SearchResult } from "./search";
import { imageThumbnailDataUrl, thumbnailDataUrl, writeDragPage } from "./pages";

const prepared = new Map<string, Promise<[string, string]>>();

/** Prépare (une seule fois) le fichier et la vignette ; appelé au survol pour un glisser immédiat. */
export function prepareDrag(r: SearchResult, ref: string) {
  const key = `${r.docPath}#${r.pageNum}#${ref}`;
  if (!prepared.has(key)) {
    const job: Promise<[string, string]> =
      r.pageNum == null
        ? Promise.all([r.docPath, r.kind === "image" ? imageThumbnailDataUrl(r.docPath) : thumbnailDataUrl(r.docPath, 1)])
        : Promise.all([writeDragPage(r.docPath, r.pageNum, ref), thumbnailDataUrl(r.docPath, r.pageNum)]);
    job.catch(() => prepared.delete(key));
    prepared.set(key, job);
  }
  return prepared.get(key)!;
}

/** Sur `mousedown` : le glisser démarre si la souris bouge de quelques pixels (un clic reste un clic). */
export function dragOnMove(event: MouseEvent, r: SearchResult, ref: string) {
  if (event.button !== 0) return;
  const job = prepareDrag(r, ref);
  const x0 = event.clientX;
  const y0 = event.clientY;
  const onMove = async (e: MouseEvent) => {
    if (Math.hypot(e.clientX - x0, e.clientY - y0) < 5) return;
    cleanup();
    try {
      const [file, icon] = await job;
      await startDrag({ item: [file], icon, mode: "copy" });
    } catch (err) {
      console.error("[docs] glisser impossible :", err);
    }
  };
  const cleanup = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", cleanup);
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", cleanup);
}
