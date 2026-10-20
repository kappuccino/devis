// Ligne de devis en cours d'édition, partagée entre l'éditeur (QuoteEditView) et le composant
// d'une ligne du tableau (QuoteLineRow).
import type { InjectionKey } from "vue";
import type { AutoCompleteCompleteEvent } from "primevue/autocomplete";
import type { PriceTier, ProductHit, QuoteLine } from "./api";
import { formatNumber, formatUnitPrice } from "./format";
import { netUnitPrice } from "./quoteLines";

/** Ligne en cours d'édition : la ligne enregistrée + ce qui sert seulement à l'affichage. */
export interface EditLine extends QuoteLine {
  key: number;
  resolvedRef: string | null;
  /** Code ENEDIS du produit trouvé (pour savoir si la colonne ENEDIS a été modifiée). */
  resolvedEnedis?: string | null;
  /** Réf en cours de résolution : évite un double appel (sélection + Entrée + blur). */
  pendingRef: string | null;
  error: string | null;
  /**
   * Texte des champs quantité, prix et remise, tel que tapé. `quantity` / `unit_price` / `discount`
   * en sont déduits à chaque touche : totaux et sous-totaux suivent la frappe, sans réécrire le champ.
   */
  qtyText: string;
  priceText: string;
  discountText: string;
  /** Frais ajouté automatiquement (conditions de vente), tant qu'on ne l'a pas modifié. */
  auto?: boolean;
  /** Frais de port suggérés (camion) et acceptés, tant qu'on ne les a pas modifiés. */
  suggested?: boolean;
  /** Prix par quantité de la liste retenue et palier appliqué (connus après un calcul de prix). */
  lpnTiers?: PriceTier[];
  lpnMinQty?: number | null;
}

/** Valeur affichée dans un champ en cours d'édition (sans séparateur de milliers). */
export const editText = (v: number | null, decimals: 2 | 3) =>
  v == null
    ? ""
    : v.toLocaleString("fr-FR", {
        useGrouping: false,
        minimumFractionDigits: decimals === 2 ? 2 : 0,
        maximumFractionDigits: decimals,
      });

/** Prix net de la ligne (remise supplémentaire comprise) sous le prix seuil. */
export const isBelowThreshold = (l: EditLine) =>
  l.threshold_price != null && netUnitPrice(l.unit_price, l.discount) < l.threshold_price;

/** Infobulle du prix LPN : la liste, et ses prix par quantité s'il y en a plusieurs. */
export function lpnTooltip(l: EditLine) {
  if (!l.lpn_list) return "Aucun prix négocié";
  const tiers = l.lpnTiers ?? [];
  if (tiers.length < 2) return `Liste ${l.lpn_list}`;
  const steps = tiers.map((t) => {
    const label = t.min_qty > 1 ? `dès ${formatNumber(t.min_qty)}` : "à l'unité";
    const applied = t.min_qty === (l.lpnMinQty ?? 0) ? " (appliqué)" : "";
    return `${formatUnitPrice(t.price)} ${label}${applied}`;
  });
  return `Liste ${l.lpn_list} · ${steps.join(" · ")}`;
}

/**
 * Actions de l'éditeur appelées depuis une ligne du tableau. Fournies une fois (provide) : des
 * fonctions qui ne changent jamais, pour qu'une ligne ne soit redessinée que si elle change.
 */
export interface LineRowActions {
  startDrag(line: EditLine, e: PointerEvent): void;
  onRowPointerDown(line: EditLine, e: PointerEvent): void;
  onSelectClick(line: EditLine, e: MouseEvent): void;
  removeLine(line: EditLine): void;
  searchEnedis(e: AutoCompleteCompleteEvent, line: EditLine): void;
  selectEnedis(line: EditLine, hit: ProductHit): void;
  commitEnedis(line: EditLine, focusNext: boolean): void;
  searchProducts(e: AutoCompleteCompleteEvent, line: EditLine): void;
  commitRef(line: EditLine): void;
  onRefBlur(line: EditLine): void;
  validateAndGoToQuantity(line: EditLine, field: "ref" | "enedis", e: KeyboardEvent): void;
  onQtyInput(line: EditLine, text: string): void;
  onQtyCommit(line: EditLine): void;
  onPriceInput(line: EditLine, text: string): void;
  onPriceBlur(line: EditLine): void;
  onDiscountInput(line: EditLine, text: string): void;
  moveInColumn(line: EditLine, column: "qty" | "price", direction: 1 | -1, e?: Event): void;
}

export const LINE_ROW_ACTIONS: InjectionKey<LineRowActions> = Symbol("lineRowActions");
