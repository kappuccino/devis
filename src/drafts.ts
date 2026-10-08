import type { PricingContext, QuoteLine } from "./api";

/**
 * Brouillon d'un devis en cours d'édition, gardé dans le stockage local de l'appli
 * pour ne rien perdre (fermeture de la fenêtre, changement d'onglet…).
 * Clé : `nouveau` pour un devis pas encore enregistré, sinon l'id du devis.
 */
export interface QuoteDraft extends Partial<PricingContext> {
  /** null : client ponctuel, ou pas encore de client. */
  client_code: string | null;
  client_name: string;
  date: string;
  notes: string;
  /** Remise globale (absente des brouillons plus anciens). */
  discount_pct?: number;
  lines: QuoteLine[];
  /** Date de la dernière modification (ISO). */
  savedAt: string;
}

const PREFIX = "devis-draft:";

export function readDraft(key: string): QuoteDraft | null {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    return raw ? (JSON.parse(raw) as QuoteDraft) : null;
  } catch {
    return null;
  }
}

export function writeDraft(key: string, draft: QuoteDraft) {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(draft));
  } catch {
    // Stockage indisponible ou plein : l'enregistrement normal reste possible.
  }
}

export function removeDraft(key: string) {
  try {
    localStorage.removeItem(PREFIX + key);
  } catch {
    // idem
  }
}
