// Pages à la volée : rendu (aperçu, vignette du glisser) et PDF d'une page.
import { exportFileName, exportPage } from "./core/index.js";
import { readFile, writeDragFile } from "./files";
import { pdfjs, PDFJS_OPTIONS } from "./pdfjs";

type PdfDoc = Awaited<ReturnType<typeof pdfjs.getDocument>["promise"]>;

// Petits caches : derniers fichiers lus et documents pdfjs ouverts.
const MAX_CACHE = 4;
const bytesCache = new Map<string, Promise<Uint8Array>>();
const docCache = new Map<string, Promise<PdfDoc>>();

function remember<T>(map: Map<string, Promise<T>>, key: string, value: Promise<T>) {
  map.set(key, value);
  if (map.size > MAX_CACHE) {
    const [oldKey, old] = map.entries().next().value as [string, Promise<T>];
    map.delete(oldKey);
    old.then((d) => (d as { loadingTask?: { destroy(): void } })?.loadingTask?.destroy?.()).catch(() => {});
  }
  return value;
}

export function getBytes(path: string) {
  return bytesCache.get(path) ?? remember(bytesCache, path, readFile(path));
}

function getPdf(path: string) {
  return (
    docCache.get(path) ??
    remember(
      docCache,
      path,
      // pdfjs détache le buffer qu'on lui donne : on lui passe une copie.
      getBytes(path).then((bytes) => pdfjs.getDocument({ ...PDFJS_OPTIONS, data: bytes.slice() }).promise),
    )
  );
}

type RenderTask = { cancel(): void; promise: Promise<void> };
const renderTasks = new WeakMap<HTMLCanvasElement, RenderTask>();

/** Rend une page dans un canvas, à la largeur CSS donnée (net sur écran Retina). */
export async function renderPage(canvas: HTMLCanvasElement, path: string, pageNum: number, cssWidth: number) {
  const doc = await getPdf(path);
  const page = await doc.getPage(pageNum);
  const base = page.getViewport({ scale: 1 });
  const ratio = window.devicePixelRatio || 1;
  const viewport = page.getViewport({ scale: (cssWidth / base.width) * ratio });
  canvas.width = Math.floor(viewport.width);
  canvas.height = Math.floor(viewport.height);
  canvas.style.width = `${cssWidth}px`;
  canvas.style.height = `${Math.floor(viewport.height / ratio)}px`;
  const task = page.render({ canvas, viewport }) as unknown as RenderTask;
  renderTasks.get(canvas)?.cancel();
  renderTasks.set(canvas, task);
  await task.promise;
}

/** Vignette PNG (data URL) d'une page, pour l'icône du glisser. */
export async function thumbnailDataUrl(path: string, pageNum: number, width = 120) {
  const canvas = document.createElement("canvas");
  await renderPage(canvas, path, pageNum, width / (window.devicePixelRatio || 1));
  return canvas.toDataURL("image/png");
}

/** PDF autonome d'une seule page. */
export async function pageBytes(path: string, pageNum: number): Promise<Uint8Array> {
  return exportPage(await getBytes(path), pageNum);
}

/** Écrit le PDF d'une page dans le dossier temporaire du glisser ; renvoie son chemin. */
export async function writeDragPage(path: string, pageNum: number, ref: string) {
  return writeDragFile(exportFileName(path, pageNum, ref), await pageBytes(path, pageNum));
}

const imageUrls = new Map<string, Promise<string>>();

/** URL (blob:) d'une image du dossier, pour l'aperçu. */
export function imageUrl(path: string) {
  if (!imageUrls.has(path)) {
    const type = /\.png$/i.test(path) ? "image/png" : "image/jpeg";
    imageUrls.set(
      path,
      readFile(path).then((bytes) => URL.createObjectURL(new Blob([bytes as BlobPart], { type }))),
    );
  }
  return imageUrls.get(path)!;
}

/** Vignette PNG (data URL) d'une image, pour l'icône du glisser. */
export async function imageThumbnailDataUrl(path: string, width = 120) {
  const img = new Image();
  img.src = await imageUrl(path);
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = Math.max(1, Math.round((img.naturalHeight / img.naturalWidth) * width));
  canvas.getContext("2d")!.drawImage(img, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/png");
}
