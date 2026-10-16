import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { buildDocument, DEFAULT_VALIDITY, HEAD_OFFICE, ORDERS_EMAIL } = await import("./usePdf");
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
  it("imprime le siège et le SIRET en pied de page, pas en haut", () => {
    const doc = buildDocument(quote, null, { company_name: "CAHORS", agency_contact: "Jean Dupont" });
    const footer = texts((doc.footer as (p: number, n: number) => unknown)(1, 1));
    const i = footer.indexOf(HEAD_OFFICE[0]);
    expect(footer.slice(i, i + 2)).toEqual(HEAD_OFFICE);
    expect(footer).toContain("1 / 1");
    expect(footer.join(" ")).not.toContain("Jean Dupont");
    expect(texts(doc.content)).not.toContain(HEAD_OFFICE[0]);
  });

  it("imprime la validité des Réglages, 90 jours par défaut", () => {
    expect(DEFAULT_VALIDITY).toBe("90 jours");
    expect(texts(buildDocument(quote, null, {}).content)).toContain("90 jours");
    expect(texts(buildDocument(quote, null, { quote_validity: " " }).content)).toContain("90 jours");
    expect(texts(buildDocument(quote, null, { quote_validity: "60 jours" }).content)).toContain("60 jours");
  });

  it("met le logo dans le pied de page (sinon le nom de la société)", () => {
    const logo = "data:image/png;base64,iVBORw0KGgo=";
    const footer = (s: Record<string, string>) => JSON.stringify((buildDocument(quote, null, s).footer as (p: number, n: number) => unknown)(2, 3));
    expect(footer({ company_logo: logo })).toContain(logo);
    expect(footer({ company_name: "GROUPE CAHORS" })).toContain("GROUPE CAHORS");
    expect(footer({})).toContain("2 / 3");
  });

  it("place les conditions générales de vente en bas à gauche, à côté des totaux", () => {
    const content = buildDocument(quote, null, {}).content as unknown as Record<string, unknown>[];
    const bottom = content[content.length - 1] as { columns: unknown[]; unbreakable: boolean };
    expect(bottom.unbreakable).toBe(true);
    expect(texts(bottom.columns[0])).toContain("CONDITIONS GÉNÉRALES DE VENTE");
    expect(texts(bottom.columns[1])).toContain("TOTAL HT");
    const left = texts(bottom.columns[0]);
    expect(left.indexOf(ORDERS_EMAIL)).toBeGreaterThan(-1);
    expect(left.indexOf(ORDERS_EMAIL)).toBeLessThan(left.indexOf("CONDITIONS GÉNÉRALES DE VENTE"));
  });

  it("n'imprime pas le nom de la société sous le logo", () => {
    const logo = "data:image/png;base64,iVBORw0KGgo=";
    expect(texts(buildDocument(quote, null, { company_name: "GROUPE CAHORS", company_logo: logo }).content)).not.toContain("GROUPE CAHORS");
    expect(texts(buildDocument(quote, null, { company_name: "GROUPE CAHORS" }).content)).toContain("GROUPE CAHORS");
  });

  it("imprime le rédacteur du devis sous le logo, avant le client", () => {
    const all = texts(
      buildDocument(quote, null, { author_name: "Julie Martin", author_address: "1 rue X\n46000 Cahors", author_phone: "05 00", author_email: "j@x.fr" }).content,
    );
    const i = all.indexOf("Julie Martin");
    expect(all.slice(i, i + 5)).toEqual(["Julie Martin", "1 rue X", "46000 Cahors", "Tél. 05 00", "j@x.fr"]);
    expect(all.indexOf("Client")).toBeGreaterThan(i);
  });

  it("imprime le nom de l'affaire, s'il y en a un", () => {
    const all = texts(buildDocument({ ...quote, project_name: " Lotissement Les Jardins " }, null, {}).content);
    expect(all).toContain("Lotissement Les Jardins");
    expect(all).not.toContain("AFFAIRE");
    expect(all).not.toContain("CLIENT");
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

  it("écrit les lignes en option en gris clair, les autres en noir", () => {
    const item = (ref: string, is_option: boolean) => ({ kind: "item", product_ref: ref, designation: "A", quantity: 2, unit_price: 10, discount: 0, is_option });
    const q = { ...quote, lines: [item("NOIR", false), item("GRIS", true)] } as unknown as Quote;
    /** Cellules du tableau des lignes qui suivent la cellule de la réf. */
    const row = (ref: string): Record<string, unknown>[] => {
      const find = (c: unknown): Record<string, unknown>[] | null => {
        if (Array.isArray(c)) {
          if (c.some((x) => x && typeof x === "object" && (x as { text?: unknown }).text === ref)) return c;
          for (const x of c) {
            const r = find(x);
            if (r) return r;
          }
          return null;
        }
        return c && typeof c === "object" ? find(Object.values(c)) : null;
      };
      return find(buildDocument(q, null, {}).content) ?? [];
    };
    const colors = (ref: string) => row(ref).map((c) => (typeof c === "object" ? c.color : undefined));
    expect(colors("GRIS").length).toBeGreaterThan(4);
    expect(new Set(colors("GRIS"))).toEqual(new Set(["#a3a8b0"]));
    expect(colors("NOIR").every((c) => c === undefined)).toBe(true);
  });
});
