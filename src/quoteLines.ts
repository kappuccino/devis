import { round2 } from "./format";

/**
 * Lignes d'un devis :
 * - `item` : un article (réf, quantité, prix) ;
 * - `title` : un titre de paragraphe (dans `designation`) ;
 * - `text` : une ligne de texte libre (dans `designation`) ;
 * - `subtotal` : sous-total des articles depuis le sous-total précédent (ou le début du devis) ;
 * - `shipping` / `billing` : frais de port / de facturation, toujours en bas, montant HT dans
 *   `unit_price`, compris dans le total HT mais jamais remisés.
 */
export type LineKind = "item" | "title" | "text" | "subtotal" | "shipping" | "billing";

export const FEE_KINDS = ["shipping", "billing"] as const;
export type FeeKind = (typeof FEE_KINDS)[number];
export const isFee = (kind: LineKind): kind is FeeKind => kind === "shipping" || kind === "billing";

export interface LineLike {
  kind: LineKind;
  quantity: number | null;
  unit_price: number;
  /** Remise supplémentaire de la ligne, en %. */
  discount?: number | null;
  /** Ligne en option : hors total HT et sous-totaux, comptée dans « Total options ». */
  is_option?: boolean;
}

/** Prix unitaire après la remise supplémentaire de la ligne, au centime. */
export const netUnitPrice = (unitPrice: number, discount: number | null | undefined) =>
  round2(unitPrice * (1 - (discount ?? 0) / 100));

export const lineTotal = (l: LineLike) =>
  l.kind === "item"
    ? round2((l.quantity ?? 0) * netUnitPrice(l.unit_price, l.discount))
    : isFee(l.kind)
      ? round2(l.unit_price)
      : 0;

/** Total après la remise globale du devis (en %). */
export const discountedTotal = (total: number, discountPct: number | null | undefined) =>
  round2(total * (1 - (discountPct ?? 0) / 100));

/**
 * Totaux du devis :
 * - `total` (Total HT) = produits hors options + frais ;
 * - `discount` = remise globale, sur les produits seulement ; `net` = total − remise ;
 * - `options` = lignes en option, à part.
 */
export function quoteTotals(lines: LineLike[], discountPct: number | null | undefined = 0) {
  let products = 0;
  let fees = 0;
  let options = 0;
  for (const l of lines) {
    if (l.kind === "item") {
      if (l.is_option) options += lineTotal(l);
      else products += lineTotal(l);
    } else if (isFee(l.kind)) {
      fees += lineTotal(l);
    }
  }
  products = round2(products);
  const total = round2(products + round2(fees));
  const discount = round2(products - discountedTotal(products, discountPct));
  return { products, fees: round2(fees), total, discount, net: round2(total - discount), options: round2(options) };
}

/** Montant de chaque ligne de sous-total (lignes en option exclues). */
export function subtotals<T extends LineLike>(lines: T[]): Map<T, number> {
  const result = new Map<T, number>();
  let running = 0;
  for (const l of lines) {
    if (l.kind === "subtotal") {
      result.set(l, round2(running));
      running = 0;
    } else if (l.kind === "item" && !l.is_option) {
      running += lineTotal(l);
    }
  }
  return result;
}

/**
 * Glisser-déposer : place `block` juste avant `target` (ou en fin si `target` est null).
 * Lâché sur une ligne du bloc lui-même : rien ne bouge.
 * `trailing` (la ligne vide de saisie) reste toujours en dernier.
 */
export function moveBlockBefore<T>(lines: T[], block: T[], target: T | null, trailing: T | null): T[] {
  const moving = lines.filter((l) => block.includes(l) && l !== trailing);
  if (!moving.length || (target && moving.includes(target))) return lines;
  const rest = lines.filter((l) => !moving.includes(l) && l !== trailing);
  const i = target && target !== trailing ? rest.indexOf(target) : -1;
  const at = i >= 0 ? i : rest.length;
  return withTrailing([...rest.slice(0, at), ...moving, ...rest.slice(at)], trailing);
}

/** Insère `block` après `anchor` (sinon en fin, avant la ligne de saisie `trailing`). */
export function insertBlock<T>(lines: T[], block: T[], anchor: T | null, trailing: T | null): T[] {
  const rest = lines.filter((l) => l !== trailing);
  const i = anchor ? rest.indexOf(anchor) : -1;
  const at = i >= 0 ? i + 1 : rest.length;
  return withTrailing([...rest.slice(0, at), ...block, ...rest.slice(at)], trailing);
}

const withTrailing = <T>(list: T[], trailing: T | null) => (trailing ? [...list, trailing] : list);

export interface CleanableLine extends LineLike {
  product_ref: string;
  designation: string;
}

/** Résultat du nettoyage : lignes gardées et lignes retirées, par motif. */
export interface CleanResult<T> {
  kept: T[];
  /** Articles (réf saisie) sans quantité, ou à 0. */
  noQuantity: T[];
  /** Lignes vides : article sans réf, titre ou texte sans libellé. */
  empty: T[];
  /** Titres, textes et sous-totaux qui n'accompagnent plus aucun article. */
  orphans: T[];
}

/**
 * Nettoyage des lignes d'un devis (frais exclus, ligne de saisie exclue) :
 * 1. retire les articles sans quantité et les lignes vides ;
 * 2. retire les paragraphes devenus vides : un titre sans article avant le titre suivant part
 *    avec ses lignes de texte et ses sous-totaux ;
 * 3. retire les sous-totaux sans article depuis le sous-total précédent (doublons compris).
 * Le texte placé avant le premier titre (introduction) est gardé.
 */
export function cleanLines<T extends CleanableLine>(lines: T[]): CleanResult<T> {
  const noQuantity: T[] = [];
  const empty: T[] = [];
  const orphans: T[] = [];

  let rest = lines.filter((l) => {
    if (l.kind === "item") {
      if (!l.product_ref.trim()) return empty.push(l), false;
      if (!l.quantity) return noQuantity.push(l), false;
    } else if ((l.kind === "title" || l.kind === "text") && !l.designation.trim()) {
      return empty.push(l), false;
    }
    return true;
  });

  // Paragraphes (d'un titre au suivant) sans article.
  const dropped = new Set<T>();
  for (let i = 0; i < rest.length; i++) {
    if (rest[i].kind !== "title") continue;
    let j = i + 1;
    while (j < rest.length && rest[j].kind !== "title") j++;
    const section = rest.slice(i, j);
    if (!section.some((l) => l.kind === "item")) section.forEach((l) => dropped.add(l));
    i = j - 1;
  }
  rest = rest.filter((l) => !dropped.has(l) || (orphans.push(l), false));

  // Sous-totaux sans article depuis le précédent.
  let items = 0;
  rest = rest.filter((l) => {
    if (l.kind === "item") items++;
    if (l.kind !== "subtotal") return true;
    const keep = items > 0;
    items = 0;
    return keep || (orphans.push(l), false);
  });

  return { kept: rest, noQuantity, empty, orphans };
}
