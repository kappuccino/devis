import type { Settings } from "./api";

/** Réglage : noms des commerciaux proposés sur les devis, un par ligne. */
export const SALES_REPS_KEY = "sales_reps";

export const salesRepList = (settings: Settings): string[] =>
  (settings[SALES_REPS_KEY] ?? "")
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);

export const salesRepSetting = (names: string[]) => names.map((s) => s.trim()).filter(Boolean).join("\n");

/** Dernier commercial choisi sur ce poste : proposé par défaut sur les nouveaux devis. */
const LAST_KEY = "devis.lastSalesRep";

export function lastSalesRep(): string {
  try {
    return localStorage.getItem(LAST_KEY) ?? "";
  } catch {
    return "";
  }
}

export function rememberSalesRep(name: string) {
  try {
    if (name) localStorage.setItem(LAST_KEY, name);
  } catch {
    // stockage indisponible : pas de valeur par défaut, sans gravité
  }
}
