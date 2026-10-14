// Frais d'après les conditions de vente (Réglages → Frais automatiques), sur le total HT des produits :
// - sous le minimum de facturation : frais de facturation et frais de port forfaitaires, ajoutés d'office ;
// - du minimum de facturation au franco : frais de port suggérés d'après le département de livraison
//   (tableau des frais de port), ajoutés seulement si on les accepte ;
// - au-delà du franco : aucun frais.
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
  // Frais de port forfaitaires, sous le minimum de facturation (même seuil).
  shipping: { amount: ["fee_shipping_amount", "31"] },
  // Franco de port : en dessous (et dès le minimum de facturation), frais de port suggérés.
  franco: ["fee_shipping_threshold", "840"],
} as const;

/** Tableau des frais de port par département (JSON), importé dans les Réglages. */
export const SHIPPING_RATES_KEY = "shipping_rates";

/** Valeur numérique d'un réglage ; vide → valeur par défaut (mettre 0 pour désactiver). */
const num = (v: string | undefined, fallback: string) => {
  const raw = v?.trim() ? v : fallback;
  const n = Number(raw.replace(",", ".").trim() || "0");
  return Number.isFinite(n) ? n : 0;
};

const setting = (settings: Settings, [key, fallback]: readonly [string, string]) => num(settings[key], fallback);

/** Règles actives (seuil et montant renseignés). */
export function feeRulesFrom(settings: Settings): FeeRules {
  const rules: FeeRules = {};
  const threshold = setting(settings, FEE_SETTINGS.billing.threshold);
  const amounts = {
    billing: setting(settings, FEE_SETTINGS.billing.amount),
    shipping: setting(settings, FEE_SETTINGS.shipping.amount),
  };
  for (const kind of ["billing", "shipping"] as const) {
    if (threshold > 0 && amounts[kind] > 0) rules[kind] = { threshold, amount: amounts[kind] };
  }
  return rules;
}

/** Tranche du total des produits où l'on suggère des frais de port (null : désactivé). */
export function shippingSuggestionRange(settings: Settings): { from: number; to: number } | null {
  const from = setting(settings, FEE_SETTINGS.billing.threshold);
  const to = setting(settings, FEE_SETTINGS.franco);
  return to > from ? { from, to } : null;
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

export interface ShippingRate {
  /** Tel qu'écrit dans le tableau (« 4 », « 4 VILLES HAUTES MONTAGNES », « 2A »…). */
  department: string;
  destination: string;
  price: number;
}

export function parseRates(json: string | undefined): ShippingRate[] {
  try {
    const rows = JSON.parse(json || "[]");
    return Array.isArray(rows) ? rows.filter((r) => r && typeof r.department === "string" && r.price > 0) : [];
  } catch {
    return [];
  }
}

/** « 04 » → « 4 », « 2a » → « 2A », « 4 VILLES HAUTES MONTAGNES » → « 4 » ; pas de numéro → null. */
const departmentKey = (s: string) => s.trim().toUpperCase().match(/^0*(\d+[AB]?)/)?.[1] ?? null;

const plain = (s: string) =>
  s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();

/**
 * Tarifs pour ce qui a été tapé : numéro de département (« 4 », « 04 », « 2A »), code postal
 * (« 04400 » → 4, « 97200 » → 972, « 20000 » → Corse) ou nom (« alpes mar »). Un département peut
 * avoir plusieurs lignes (ex. « 4 » et « 4 VILLES HAUTES MONTAGNES »).
 */
export function ratesFor(rates: ShippingRate[], typed: string): ShippingRate[] {
  const t = typed.trim();
  if (!t) return [];
  let keys: string[];
  if (/^\d{5}$/.test(t)) {
    const dep = t.startsWith("97") || t.startsWith("98") ? t.slice(0, 3) : t.slice(0, 2);
    keys = dep === "20" ? ["2A", "2B", "20"] : [String(Number(dep))];
  } else {
    const key = departmentKey(t);
    if (!key) {
      const q = plain(t);
      return rates.filter((r) => plain(r.destination).includes(q) || plain(r.department).includes(q));
    }
    // Corse : le tableau peut n'avoir qu'une ligne « 20 ».
    keys = key === "2A" || key === "2B" ? [key, "20"] : [key];
  }
  return rates.filter((r) => keys.includes(departmentKey(r.department) ?? ""));
}
