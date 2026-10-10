// Conditions générales de vente imprimées en bas de tous les devis : identiques pour tout le
// monde, fixées ici (plus de réglage). Mise en forme simple : une ligne « # … » est un titre ;
// **…** en gras ; __…__ souligné.

export const CONDITIONS = `**VIREMENT SUR FACTURE 30 JOURS FIN DE MOIS LE 10**

**Garantie contractuelle :** 1 an, pièces et main d'oeuvre en nos ateliers
Nos prix sont nets, unitaires, Hors Taxes.

**Emballage et transport :** Franco de port et d'emballage pour toute commande de plus de 840 EUR HT et livraison en un seul point.

**Minimum de facturation :** Toute commande inférieure à 140 EUR HT sera majorée de 25 EUR pour participation aux frais de facturation et de traitement.

**Toute commande entraîne de plein droit l'acceptation de nos conditions générales de vente.**`;

export interface Run {
  text: string;
  bold?: boolean;
  underline?: boolean;
}

export interface ConditionLine {
  title: boolean;
  runs: Run[];
}

/** Découpe le texte en lignes (titre ou non) et en morceaux gras / soulignés. */
export function parseConditions(text: string): ConditionLine[] {
  return text.split(/\r?\n/).map((raw) => {
    const title = raw.startsWith("# ");
    const body = title ? raw.slice(2) : raw;
    const runs: Run[] = [];
    for (const part of body.split(/(\*\*[^*]+\*\*|__[^_]+__)/)) {
      if (!part) continue;
      if (part.startsWith("**") && part.endsWith("**")) runs.push({ text: part.slice(2, -2), bold: true });
      else if (part.startsWith("__") && part.endsWith("__")) runs.push({ text: part.slice(2, -2), underline: true });
      else runs.push({ text: part });
    }
    return { title, runs };
  });
}
