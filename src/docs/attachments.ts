// PDF « Devis + Docs » : propositions de documentation pour chaque produit du devis,
// choix mémorisés avec le devis, et assemblage du PDF final
// (devis → page de transition → documents, dans l'ordre des produits du devis).
import { PDFDocument } from "pdf-lib";
import { api, type QuoteAttachment, type QuoteLine } from "../api";
import { transitionPdfBytes } from "../composables/usePdf";
import { fileKind } from "./core/index.js";
import { fileName, readFile } from "./files";
import { searchDocs, type SearchResult } from "./search";

/** Une page (ou un fichier entier) proposée pour un produit. */
export interface Proposal {
  key: string;
  result: SearchResult;
  included: boolean;
}

/** Produit du devis et sa documentation trouvée. */
export interface ProductGroup {
  ref: string;
  designation: string;
  /** Correspondances exactes, début de référence, nom de fichier. */
  proposals: Proposal[];
  /** Correspondances approximatives (souvent du bruit) : proposées à part, décochées. */
  approx: Proposal[];
}

/** Document ajouté à la main (PDF ou image), joint en entier. */
export interface ExternalDoc {
  key: string;
  path: string;
  included: boolean;
  /** Fichier déplacé ou supprimé depuis : signalé et ignoré à la génération. */
  missing: boolean;
}

const keyOf = (path: string, page: number | null) => `${path}#${page ?? "all"}`;

/** Résultat de recherche reconstitué pour un document mémorisé (aperçu, génération). */
function savedResult(path: string, page: number | null): SearchResult {
  return {
    docPath: path,
    kind: fileKind(path) === "image" ? "image" : "pdf",
    pageNum: page,
    pageCount: 0,
    createdAt: null,
    nameMatch: false,
    nameHighlight: null,
    match: "exact",
    snippet: "",
    highlight: null,
  };
}

/**
 * Propositions pour chaque produit du devis (référence, puis code ENEDIS), dans l'ordre du devis.
 * Rien n'est coché par défaut ; les choix déjà enregistrés pour ce devis sont repris.
 */
export async function buildProposals(lines: QuoteLine[], saved: QuoteAttachment[]) {
  const savedIndex = new Map(saved.filter((a) => a.source === "index").map((a) => [keyOf(a.path, a.page_num), a]));
  const seen = new Set<string>();
  const groups: ProductGroup[] = [];

  const products = lines.filter((l) => l.kind === "item" && l.product_ref.trim());
  for (const line of products) {
    if (groups.some((g) => g.ref === line.product_ref)) continue;
    const queries = [line.product_ref, line.enedis_code].filter((q): q is string => !!q && q.trim().length >= 3);
    const found = (await Promise.all(queries.map((q) => searchDocs(q)))).flat();

    const group: ProductGroup = { ref: line.product_ref, designation: line.designation, proposals: [], approx: [] };
    for (const result of found) {
      const key = keyOf(result.docPath, result.pageNum);
      if (seen.has(key)) continue; // une même page n'est proposée qu'une fois (au premier produit)
      seen.add(key);
      const choice = savedIndex.get(key);
      const proposal = { key, result, included: choice?.included ?? false };
      (result.match === "partial" ? group.approx : group.proposals).push(proposal);
    }
    groups.push(group);
  }

  // Documents choisis précédemment mais que la recherche ne retrouve plus : gardés (s'ils existent encore).
  const forgotten = saved.filter((a) => a.source === "index" && !seen.has(keyOf(a.path, a.page_num)));
  const stillThere = await api.filesExist(forgotten.map((a) => a.path));
  forgotten.forEach((a, i) => {
    const group = groups.find((g) => g.ref === a.product_ref);
    if (!group || !stillThere[i]) return;
    const key = keyOf(a.path, a.page_num);
    seen.add(key);
    group.proposals.push({ key, result: savedResult(a.path, a.page_num), included: a.included });
  });

  const externalSaved = saved.filter((a) => a.source === "external");
  const exists = await api.filesExist(externalSaved.map((a) => a.path));
  const externals: ExternalDoc[] = externalSaved.map((a, i) => ({
    key: keyOf(a.path, null),
    path: a.path,
    included: a.included,
    missing: !exists[i],
  }));

  return { groups, externals };
}

/** Nouveau document ajouté à la main. */
export const externalDoc = (path: string): ExternalDoc => ({ key: keyOf(path, null), path, included: true, missing: false });

/** Choix à mémoriser avec le devis : les propositions cochées et les documents ajoutés à la main. */
export function toAttachments(groups: ProductGroup[], externals: ExternalDoc[]): QuoteAttachment[] {
  const out: QuoteAttachment[] = [];
  for (const g of groups) {
    for (const p of [...g.proposals, ...g.approx]) {
      if (!p.included) continue;
      out.push({ source: "index", product_ref: g.ref, path: p.result.docPath, page_num: p.result.pageNum, included: p.included });
    }
  }
  for (const e of externals) {
    out.push({ source: "external", product_ref: "", path: e.path, page_num: null, included: e.included });
  }
  return out;
}

/** Rendu en image des pages qu'on ne peut pas recopier (fourni par l'appli : pdfjs + canvas). */
export interface Rasterizer {
  pageCount(path: string): Promise<number>;
  page(path: string, pageNum: number): Promise<{ png: Uint8Array; width: number; height: number }>;
}

/** Élément à joindre au PDF final. */
interface Part {
  path: string;
  /** Page du fichier, ou null pour le fichier entier. */
  pageNum: number | null;
}

/** Parties à joindre, dans l'ordre : produits du devis, puis documents ajoutés. */
export function includedParts(groups: ProductGroup[], externals: ExternalDoc[]): Part[] {
  const parts: Part[] = [];
  for (const g of groups) {
    for (const p of [...g.proposals, ...g.approx]) {
      if (p.included) parts.push({ path: p.result.docPath, pageNum: p.result.pageNum });
    }
  }
  for (const e of externals) {
    if (e.included && !e.missing) parts.push({ path: e.path, pageNum: null });
  }
  return parts;
}

/**
 * Assemble le PDF final : devis, page de transition, documents.
 * Les pages sont recopiées telles quelles ; si un PDF est mal formé, ses pages sont rendues en image
 * (`rasterizer`). Un document illisible malgré tout est ignoré et signalé dans `warnings`.
 */
export async function assembleQuoteWithDocs(quotePdf: Uint8Array, parts: Part[], rasterizer?: Rasterizer) {
  const out = await PDFDocument.create();
  const warnings: string[] = [];
  const sources = new Map<string, Promise<PDFDocument>>();
  const load = (path: string) => {
    if (!sources.has(path)) {
      sources.set(path, readFile(path).then((b) => PDFDocument.load(b, { ignoreEncryption: true, updateMetadata: false })));
    }
    return sources.get(path)!;
  };

  const quote = await PDFDocument.load(quotePdf);

  // Parties lisibles (illisibles écartées et signalées), avec leur nombre de pages.
  // `raster` : pages à rendre en image (PDF que pdf-lib ne sait pas recopier).
  const sized: { part: Part; pages: number; raster: boolean }[] = [];
  const failed = (part: Part, e: unknown) => warnings.push(`${fileName(part.path)} : ${e instanceof Error ? e.message : String(e)}`);
  for (const part of parts) {
    if (fileKind(part.path) === "image") {
      sized.push({ part, pages: 1, raster: false });
      continue;
    }
    try {
      const src = await load(part.path);
      // Vérifie que les pages se recopient (certains PDF mal formés se chargent mais pas leurs pages).
      const indices = part.pageNum != null ? [part.pageNum - 1] : src.getPageIndices();
      await (await PDFDocument.create()).copyPages(src, indices);
      sized.push({ part, pages: indices.length, raster: false });
    } catch (e) {
      if (!rasterizer) {
        failed(part, e);
        continue;
      }
      try {
        const pages = part.pageNum != null ? 1 : await rasterizer.pageCount(part.path);
        sized.push({ part, pages, raster: true });
      } catch (e2) {
        failed(part, e2);
      }
    }
  }

  const transition = await PDFDocument.load(await transitionPdfBytes());

  for (const src of [quote, transition]) {
    for (const page of await out.copyPages(src, src.getPageIndices())) out.addPage(page);
  }

  for (const { part, pages, raster } of sized) {
    try {
      if (fileKind(part.path) === "image") {
        await addImagePage(out, await readFile(part.path), part.path);
        continue;
      }
      if (raster && rasterizer) {
        const nums = part.pageNum != null ? [part.pageNum] : Array.from({ length: pages }, (_, i) => i + 1);
        for (const n of nums) {
          const { png, width, height } = await rasterizer.page(part.path, n);
          const image = await out.embedPng(png);
          out.addPage([width, height]).drawImage(image, { x: 0, y: 0, width, height });
        }
        continue;
      }
      const src = await load(part.path);
      const indices = part.pageNum != null ? [part.pageNum - 1] : src.getPageIndices();
      for (const page of await out.copyPages(src, indices)) out.addPage(page);
    } catch (e) {
      warnings.push(`${fileName(part.path)} : ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return { bytes: await out.save(), warnings };
}

/** Image sur une page A4, centrée et réduite pour tenir dans les marges. */
async function addImagePage(out: PDFDocument, bytes: Uint8Array, path: string) {
  const image = /\.png$/i.test(path) ? await out.embedPng(bytes) : await out.embedJpg(bytes);
  const [w, h] = [595.28, 841.89];
  const margin = 36;
  const scale = Math.min((w - 2 * margin) / image.width, (h - 2 * margin) / image.height, 1);
  const page = out.addPage([w, h]);
  const dw = image.width * scale;
  const dh = image.height * scale;
  page.drawImage(image, { x: (w - dw) / 2, y: (h - dh) / 2, width: dw, height: dh });
}
