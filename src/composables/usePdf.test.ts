import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

const { agencyContact, AGENCY_KEY, buildDocument, HEAD_OFFICE } = await import("./usePdf");
import type { Quote } from "../api";
import { formatEuro } from "../format";

const quote = { number: "26-TEST-0001", date: "2026-10-09", client_name: "Client", lines: [], total_ht: 0, total_net: 0, discount_pct: 0, total_options: 0 } as unknown as Quote;

/** Tous les textes (`text`) d'un contenu pdfmake, à plat. */
const texts = (c: unknown): string[] => {
  if (Array.isArray(c)) return c.flatMap(texts);
  if (!c || typeof c !== "object") return [];
  const { text, ...rest } = c as Record<string, unknown>;
  return [...(typeof text === "string" ? [text] : texts(text)), ...Object.values(rest).flatMap(texts)];
};

describe("PDF du devis", () => {
  it("imprime le siège, puis les coordonnées de l'agence", () => {
    const doc = buildDocument(quote, null, { company_name: "CAHORS", [AGENCY_KEY]: "Jean Dupont\nTél. 06 00 00 00 00" });
    const all = texts(doc.content);
    const i = all.indexOf("CAHORS");
    expect(all.slice(i, i + 4)).toEqual(["CAHORS", ...HEAD_OFFICE, "Jean Dupont\nTél. 06 00 00 00 00"]);
  });

  it("n'imprime pas le nom de la société sous le logo", () => {
    const logo = "data:image/png;base64,iVBORw0KGgo=";
    expect(texts(buildDocument(quote, null, { company_name: "GROUPE CAHORS", company_logo: logo }).content)).not.toContain("GROUPE CAHORS");
    expect(texts(buildDocument(quote, null, { company_name: "GROUPE CAHORS" }).content)).toContain("GROUPE CAHORS");
  });

  it("reprend les anciens champs tant que l'agence n'a pas été saisie", () => {
    expect(agencyContact({ company_address: "1 rue X", company_phone: "05 00", company_email: "a@b.fr" })).toBe("1 rue X\nTél. 05 00\na@b.fr");
    expect(agencyContact({ company_address: "1 rue X", [AGENCY_KEY]: "" })).toBe("");
  });

  it("totaux dans l'ordre du calcul : produits, remise, frais, TOTAL HT", () => {
    const item = (price: number, is_option = false) => ({ kind: "item", product_ref: "R", designation: "A", quantity: 1, unit_price: price, discount: 0, is_option });
    const q = {
      ...quote,
      discount_pct: 10,
      lines: [item(100), item(50, true), { kind: "shipping", designation: "Frais de port", unit_price: 31 }, { kind: "billing", designation: "Frais de facturation", unit_price: 25 }],
    } as unknown as Quote;
    const all = texts(buildDocument(q, null, {}).content);
    const i = all.indexOf("Total produits HT");
    expect(all.slice(i, i + 12)).toEqual([
      "Total produits HT", formatEuro(100),
      "Remise 10 % sur les produits", `− ${formatEuro(10)}`,
      "Total produits remisé HT", formatEuro(90),
      "Frais de port", formatEuro(31),
      "Frais de facturation", formatEuro(25),
      "TOTAL HT", formatEuro(146),
    ]);
  });
});
