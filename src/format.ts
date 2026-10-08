const euro = new Intl.NumberFormat("fr-FR", { style: "currency", currency: "EUR" });
const number = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 3 });

// Intl sépare les milliers par une espace fine insécable (U+202F), absente de la police du PDF.
const fmt = (f: Intl.NumberFormat) => (v: number | null | undefined) =>
  v == null ? "" : f.format(v).replace(/\u202f/g, "\u00a0");

export const formatEuro = fmt(euro);
/** Tous les prix s'affichent au centime. */
export const formatUnitPrice = formatEuro;
export const formatNumber = fmt(number);
export const formatPct = (v: number | null | undefined) => (v == null ? "" : `${number.format(v)} %`);

export function formatDate(iso: string) {
  const [y, m, d] = iso.split("-");
  return d && m && y ? `${d}/${m}/${y}` : iso;
}

export function todayIso() {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Arrondi au centime, sans les erreurs du binaire (1,005 → 1,01 et non 1,00). */
export const round2 = (v: number) => Math.round(Math.round(v * 1e9) / 1e7) / 100;

export function errorMessage(e: unknown) {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
