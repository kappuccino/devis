// Indexation d'un dossier : liste des fichiers (Rust) + extraction (worker pdfjs) + écriture (cœur).
import { createExtractor, fileKind, indexDocument, indexImage, listDocs, removeDoc } from "./core/index.js";
import type { DbAdapter } from "./db";
import { listFiles, readFile } from "./files";
import { pdfjs, PDFJS_OPTIONS } from "./pdfjs";

// L'analyse des PDF se fait dans le worker de pdfjs : l'interface reste fluide,
// seul le texte extrait revient sur le thread principal.
const extractor = createExtractor(pdfjs, PDFJS_OPTIONS);

export interface IndexProgress {
  index: number;
  total: number;
  file: string;
  pageNum?: number;
  pageCount?: number;
}

export interface IndexSummary {
  files: number;
  images: number;
  reused: number;
  added: number;
  updated: number;
  skipped: number;
  removed: number;
  pages: number;
  errors: { file: string; message: string }[];
  seconds: number;
}

/**
 * Indexe (incrémental, par SHA-1) les PDF (texte) et les images (nom seul) du dossier,
 * et retire de l'index ce qui n'y est plus.
 */
export async function indexFolder(db: DbAdapter, dir: string, onProgress: (p: IndexProgress) => void): Promise<IndexSummary> {
  const t0 = performance.now();
  const files = await listFiles(dir);
  const summary: IndexSummary = {
    files: files.length,
    images: 0,
    reused: 0,
    added: 0,
    updated: 0,
    skipped: 0,
    removed: 0,
    pages: 0,
    errors: [],
    seconds: 0,
  };
  type Status = "added" | "updated" | "skipped";

  for (const [index, { path, createdAt }] of files.entries()) {
    onProgress({ index, total: files.length, file: path });
    try {
      if (fileKind(path) === "image") {
        summary.images++;
        summary[(await indexImage(db, { path, createdAt })).status as Status]++;
        continue;
      }
      const bytes = await readFile(path);
      const res = await indexDocument(db, { path, bytes, createdAt }, extractor, {
        onPage: (pageNum: number, pageCount: number) => onProgress({ index, total: files.length, file: path, pageNum, pageCount }),
      });
      summary[res.status as Status]++;
      if (res.reused) summary.reused++;
      else if (res.status !== "skipped") summary.pages += res.pageCount;
    } catch (e) {
      summary.errors.push({ file: path, message: e instanceof Error ? e.message : String(e) });
      console.warn("[docs] ignoré :", path, e);
    }
  }

  // Chemins comparés en NFC (macOS renvoie du NFD) pour repérer les fichiers supprimés.
  const present = new Set(files.map((f) => f.path.normalize("NFC")));
  for (const doc of await listDocs(db)) {
    if (!present.has(doc.path)) {
      await removeDoc(db, doc.id);
      summary.removed++;
    }
  }
  summary.seconds = (performance.now() - t0) / 1000;
  return summary;
}
