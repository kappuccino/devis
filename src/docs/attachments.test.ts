import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { PDFDocument, StandardFonts } from "pdf-lib";
import { beforeAll, describe, expect, it, vi } from "vitest";
import type { SearchResult } from "./search";

// Lecture des fichiers : directement sur le disque (pas de Tauri dans les tests).
vi.mock("./files", async (orig) => ({
  ...(await orig<typeof import("./files")>()),
  readFile: async (p: string) => new Uint8Array(await readFile(p)),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { assembleQuoteWithDocs, includedParts, toAttachments } = await import("./attachments");

/** PDF de n pages, chaque page portant son libellé. */
async function pdf(label: string, pages: number) {
  const doc = await PDFDocument.create();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  for (let i = 1; i <= pages; i++) doc.addPage().drawText(`${label} ${i}`, { x: 50, y: 700, size: 20, font });
  return doc.save();
}

// PNG 1×1 pixel
const PNG = Uint8Array.from(
  atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg=="),
  (c) => c.charCodeAt(0),
);

let dir: string;
const file = (name: string) => path.join(dir, name);
const result = (docPath: string, pageNum: number | null, match: SearchResult["match"] = "exact"): SearchResult => ({
  docPath,
  kind: docPath.endsWith(".png") ? "image" : "pdf",
  pageNum,
  pageCount: 0,
  createdAt: null,
  nameMatch: false,
  nameHighlight: null,
  match,
  snippet: "",
  highlight: null,
});

beforeAll(async () => {
  dir = await mkdtemp(path.join(tmpdir(), "devis-docs-"));
  await writeFile(file("catalogue.pdf"), await pdf("Catalogue", 3));
  await writeFile(file("notice.pdf"), await pdf("Notice", 2));
  await writeFile(file("photo.png"), PNG);
});

describe("PDF devis + documentation", () => {
  const groups = () => [
    {
      ref: "00188",
      designation: "E4R 10-35",
      proposals: [
        { key: "a", result: result(file("catalogue.pdf"), 2), included: true },
        { key: "b", result: result(file("notice.pdf"), null, "filename"), included: true },
        { key: "c", result: result(file("catalogue.pdf"), 3, "exact"), included: false },
      ],
      approx: [{ key: "d", result: result(file("catalogue.pdf"), 1, "partial"), included: false }],
    },
    { ref: "00190", designation: "Sans doc", proposals: [], approx: [] },
  ];
  const externals = () => [
    { key: "e", path: file("photo.png"), included: true, missing: false },
    { key: "f", path: file("absent.pdf"), included: true, missing: true },
  ];

  it("enchaîne devis, page de transition puis documents cochés, dans l'ordre du devis", async () => {
    const quote = await pdf("Devis", 2);
    const parts = includedParts(groups(), externals());
    // page 2 du catalogue, notice entière, photo ; le fichier introuvable est ignoré
    expect(parts.map((p) => [path.basename(p.path), p.pageNum])).toEqual([
      ["catalogue.pdf", 2],
      ["notice.pdf", null],
      ["photo.png", null],
    ]);
    const { bytes, warnings } = await assembleQuoteWithDocs(quote, parts);
    expect(warnings).toEqual([]);
    const out = await PDFDocument.load(bytes);
    // 2 (devis) + 1 (transition) + 1 (page du catalogue) + 2 (notice) + 1 (photo)
    expect(out.getPageCount()).toBe(7);
  });

  it("mémorise les choix : cochés, et les exacts décochés (pour ne pas les recocher)", () => {
    const saved = toAttachments(groups(), externals());
    expect(saved.map((a) => [path.basename(a.path), a.page_num, a.included])).toEqual([
      ["catalogue.pdf", 2, true],
      ["notice.pdf", null, true],
      ["catalogue.pdf", 3, false], // exact décoché : mémorisé
      // approximatif décoché : non mémorisé (décoché par défaut)
      ["photo.png", null, true],
      ["absent.pdf", null, true],
    ]);
  });

  it("rend en image les pages d'un PDF que pdf-lib ne sait pas recopier", async () => {
    await writeFile(file("abime.pdf"), "pas vraiment un pdf");
    const rasterizer = {
      pageCount: async () => 3,
      page: async () => ({ png: PNG, width: 300, height: 400 }),
    };
    const { bytes, warnings } = await assembleQuoteWithDocs(
      await pdf("Devis", 1),
      [{ path: file("abime.pdf"), pageNum: null }],
      rasterizer,
    );
    expect(warnings).toEqual([]);
    const out = await PDFDocument.load(bytes);
    expect(out.getPageCount()).toBe(5); // devis + transition + 3 pages en image
    expect(out.getPage(4).getSize()).toEqual({ width: 300, height: 400 });
  });

  it("ignore et signale un document illisible", async () => {
    await writeFile(file("casse.pdf"), "pas un pdf");
    const { bytes, warnings } = await assembleQuoteWithDocs(await pdf("Devis", 1), [{ path: file("casse.pdf"), pageNum: null }]);
    expect(warnings).toHaveLength(1);
    expect((await PDFDocument.load(bytes)).getPageCount()).toBe(2); // devis + transition
  });
});
