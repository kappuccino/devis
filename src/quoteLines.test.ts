import { describe, expect, it } from "vitest";
import {
  cleanLines,
  discountedTotal,
  insertBlock,
  lineTotal,
  moveBlockBefore,
  netUnitPrice,
  quoteTotals,
  subtotals,
  type LineLike,
} from "./quoteLines";
import { round2 } from "./format";

type L = LineLike & { n: string };
const line = (n: string, kind: L["kind"] = "item", quantity = 1, unit_price = 10): L => ({ n, kind, quantity, unit_price });
const names = (list: L[]) => list.map((l) => l.n).join(" ");

const a = line("a"), b = line("b"), t = line("t", "text"), c = line("c"), d = line("d"), blank = line("_");
const before = [a, b, t, c, d, blank];

describe("moveBlockBefore", () => {
  const move = (block: L[], target: L | null) => names(moveBlockBefore(before, block, target, blank));

  it("remonte une ligne", () => {
    expect(move([d], b)).toBe("a d b t c _");
  });
  it("descend une ligne de texte", () => {
    expect(move([t], d)).toBe("a b c t d _");
  });
  it("place en fin de devis, avant la ligne de saisie", () => {
    expect(move([a], null)).toBe("b t c d a _");
    expect(move([a], blank)).toBe("b t c d a _");
  });
  it("déplace plusieurs lignes ensemble, dans l'ordre du devis", () => {
    expect(move([c, a], d)).toBe("b t a c d _");
  });
  it("lâché sur sa propre sélection : ne change rien", () => {
    expect(move([b, t], t)).toBe("a b t c d _");
  });
});

describe("insertBlock", () => {
  it("colle juste après l'ancre", () => {
    expect(names(insertBlock(before, [line("x")], b, blank))).toBe("a b x t c d _");
  });
  it("colle en fin de devis sans ancre", () => {
    expect(names(insertBlock(before, [line("x")], null, blank))).toBe("a b t c d x _");
  });
});

describe("subtotals", () => {
  it("additionne les articles depuis le sous-total précédent", () => {
    const s1 = line("s1", "subtotal"), s2 = line("s2", "subtotal"), s3 = line("s3", "subtotal");
    const totals = subtotals([line("a", "item", 2, 5.5), line("t", "text"), s1, line("b", "item", 3, 1), s2, s3]);
    expect(totals.get(s1)).toBe(11);
    expect(totals.get(s2)).toBe(3);
    expect(totals.get(s3)).toBe(0);
  });
});

describe("remises", () => {
  it("applique la remise de ligne au prix unitaire, puis la quantité", () => {
    expect(netUnitPrice(12.35, 10)).toBe(11.12);
    expect(lineTotal({ kind: "item", quantity: 3, unit_price: 12.35, discount: 10 })).toBe(33.36);
    expect(lineTotal({ kind: "item", quantity: 3, unit_price: 12.35 })).toBe(37.05);
  });
  it("applique la remise globale au total", () => {
    expect(discountedTotal(1243.78, 3.5)).toBe(1200.25);
    expect(discountedTotal(1000, null)).toBe(1000);
  });
  it("les sous-totaux tiennent compte des remises de ligne", () => {
    const s = line("s", "subtotal");
    const totals = subtotals([{ ...line("a", "item", 2, 10), discount: 50 }, s]);
    expect(totals.get(s)).toBe(10);
  });
});

describe("options", () => {
  it("les lignes en option sortent du total HT et des sous-totaux", () => {
    const s = line("s", "subtotal");
    const lines = [line("a", "item", 2, 10), { ...line("o", "item", 1, 5), is_option: true }, s];
    expect(quoteTotals(lines)).toMatchObject({ total: 20, options: 5 });
    expect(subtotals(lines).get(s)).toBe(20);
  });
});

describe("frais de port et de facturation", () => {
  it("sont dans le total HT mais pas remisés", () => {
    const lines = [line("a", "item", 2, 50), line("port", "shipping", 0, 25), line("fact", "billing", 0, 5)];
    expect(quoteTotals(lines, 10)).toEqual({ products: 100, fees: 30, total: 130, discount: 10, net: 120, options: 0 });
  });
  it("ne comptent pas dans les sous-totaux", () => {
    const s = line("s", "subtotal");
    expect(subtotals([line("a", "item", 1, 10), line("port", "shipping", 0, 25), s]).get(s)).toBe(10);
  });
});

describe("round2", () => {
  it("arrondit au centime sans erreur binaire", () => {
    expect(round2(1.005)).toBe(1.01);
    expect(round2(2.838928)).toBe(2.84);
    expect(round2(45.11)).toBe(45.11);
  });
});

describe("cleanLines", () => {
  type C = L & { product_ref: string; designation: string };
  const c = (n: string, kind: L["kind"], quantity: number | null = 1, designation = n, product_ref = n): C =>
    ({ n, kind, quantity, unit_price: 10, designation, product_ref }) as C;
  const cnames = (list: C[]) => list.map((l) => l.n).join(" ");

  it("retire les articles sans quantité, les lignes vides et ce qui devient orphelin", () => {
    const lines = [
      c("intro", "text"),
      c("T1", "title"), c("t1", "text"), c("a", "item"), c("z0", "item", 0), c("s1", "subtotal"),
      c("T2", "title"), c("t2", "text"), c("b0", "item", null), c("s2", "subtotal"),
      c("T3", "title"), c("vide", "text", 0, "  "), c("c", "item", 2), c("_", "item", null, "", ""),
      c("s3", "subtotal"), c("s4", "subtotal"),
    ];
    const r = cleanLines(lines);
    expect(cnames(r.kept)).toBe("intro T1 t1 a s1 T3 c s3");
    expect(cnames(r.noQuantity)).toBe("z0 b0");
    expect(cnames(r.empty)).toBe("vide _");
    // Paragraphe T2 sans article (titre, texte, sous-total) ; sous-total en double.
    expect(cnames(r.orphans)).toBe("T2 t2 s2 s4");
  });

  it("ne touche à rien sur un devis propre", () => {
    const lines = [c("T1", "title"), c("a", "item"), c("s1", "subtotal"), c("b", "item", 3)];
    const r = cleanLines(lines);
    expect(r.kept).toEqual(lines);
    expect(r.noQuantity.length + r.empty.length + r.orphans.length).toBe(0);
  });
});
