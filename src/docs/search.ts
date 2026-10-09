// Recherche dans la documentation (cœur) + types partagés.
import { search as coreSearch } from "./core/index.js";
import { openIndex } from "./db";

export interface SearchResult {
  docPath: string;
  kind: "pdf" | "image";
  /** Page trouvée, ou null pour le fichier entier (image, ou PDF trouvé seulement par son nom). */
  pageNum: number | null;
  pageCount: number;
  createdAt: number | null;
  /** La référence figure dans le nom du fichier. */
  nameMatch: boolean;
  nameHighlight: { start: number; end: number } | null;
  /** Qualité : 'exact', 'prefix' (début de réf.), 'partial' (approximatif), 'filename' (nom seul). */
  match: "exact" | "prefix" | "partial" | "filename";
  snippet: string;
  highlight: { start: number; end: number } | null;
}

/** Recherche une référence (3 caractères minimum) dans le texte des PDF et le nom des fichiers. */
export async function searchDocs(query: string): Promise<SearchResult[]> {
  const { db, info } = await openIndex();
  return coreSearch(db, query, { refMode: info.refMode }) as Promise<SearchResult[]>;
}
