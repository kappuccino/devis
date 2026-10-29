/**
 * Ordre des résultats d'une recherche de clients : d'abord ceux dont la raison sociale commence par
 * le texte tapé, puis ceux dont un mot de la raison sociale ou le code commence par ce texte, puis
 * les autres. Dans chaque groupe, l'ordre d'origine est gardé.
 */
export function prefixFirst<T extends { code: string; name: string }>(items: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  const rank = (c: T) => {
    const name = c.name.toLowerCase();
    if (name.startsWith(q)) return 0;
    if (name.split(/[\s\-'’&/.,()]+/).some((w) => w.startsWith(q)) || c.code.toLowerCase().startsWith(q)) return 1;
    return 2;
  };
  return items
    .map((c, i) => ({ c, i, r: rank(c) }))
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .map((x) => x.c);
}
