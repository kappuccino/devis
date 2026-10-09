// Frais proposés automatiquement d'après les conditions de vente (Réglages → Config PDF) :
// - frais de facturation si le total des produits est sous le minimum de facturation ;
// - frais de port si le total est sous le franco (seulement si un montant est renseigné).
import type { Settings } from "./api";
import type { FeeKind } from "./quoteLines";

export interface FeeRule {
  /** Le frais s'applique sous ce total HT des produits. */
  threshold: number;
  amount: number;
}

export type FeeRules = Partial<Record<FeeKind, FeeRule>>;

/** Clés des réglages et valeurs par défaut (conditions de vente actuelles). */
export const FEE_SETTINGS = {
  billing: { threshold: ["fee_billing_threshold", "140"], amount: ["fee_billing_amount", "25"] },
  shipping: { threshold: ["fee_shipping_threshold", "840"], amount: ["fee_shipping_amount", ""] },
} as const;

/** Valeur numérique d'un réglage ; vide → valeur par défaut (mettre 0 pour désactiver). */
const num = (v: string | undefined, fallback: string) => {
  const raw = v?.trim() ? v : fallback;
  const n = Number(raw.replace(",", ".").trim() || "0");
  return Number.isFinite(n) ? n : 0;
};

/** Règles actives (seuil et montant renseignés). */
export function feeRulesFrom(settings: Settings): FeeRules {
  const rules: FeeRules = {};
  for (const kind of ["billing", "shipping"] as const) {
    const { threshold: [tk, td], amount: [ak, ad] } = FEE_SETTINGS[kind];
    const rule = { threshold: num(settings[tk], td), amount: num(settings[ak], ad) };
    if (rule.threshold > 0 && rule.amount > 0) rules[kind] = rule;
  }
  return rules;
}

/**
 * Frais automatiques à ajouter ou retirer pour un total de produits donné.
 * - ajouté s'il est dû, absent, et pas retiré à la main sur ce devis (`dismissed`) ;
 * - retiré s'il n'est plus dû et qu'il avait été ajouté automatiquement (non modifié depuis).
 */
export function autoFeeChanges(
  products: number,
  rules: FeeRules,
  present: { kind: FeeKind; auto: boolean }[],
  dismissed: ReadonlySet<FeeKind>,
) {
  const add: FeeKind[] = [];
  const remove: FeeKind[] = [];
  for (const kind of ["shipping", "billing"] as const) {
    const rule = rules[kind];
    if (!rule) continue;
    const due = products > 0 && products < rule.threshold;
    const line = present.find((l) => l.kind === kind);
    if (due && !line && !dismissed.has(kind)) add.push(kind);
    if (!due && line?.auto) remove.push(kind);
  }
  return { add, remove };
}
