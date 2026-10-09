import { round2 } from "./format";

/**
 * Lignes d'un devis :
 * - `item` : un article (réf, quantité, prix) ;
 * - `text` : une ligne de texte libre (dans `designation`) ;
 * - `subtotal` : sous-total des articles depuis le sous-total précédent (ou le début du devis) ;
 * - `shipping` / `billing` : frais de port / de facturation, toujours en bas, montant HT dans
 *   `unit_price`, compris dans le total HT mais jamais remisés.
 */
export type LineKind = "item" | "text" | "subtotal" | "shipping" | "billing";

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
