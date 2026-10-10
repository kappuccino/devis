// Ordre de tabulation du devis « par colonne » : Tab descend d'une ligne dans la même colonne,
// Maj+Tab remonte (ordre natif du navigateur, via tabindex). En bas d'une colonne, Tab repart
// en haut de la colonne suivante ; les champs des totaux viennent en dernier.

/** Colonne où ranger les champs pleine largeur (titre, texte, sous-total) : la Désignation. */
export const WIDE_COLUMN = 4;
const ROWS_PER_COLUMN = 1000;
const TOTALS_BASE = 100 * ROWS_PER_COLUMN;

/** tabindex d'un champ de la ligne `row` (0, 1…) dans la colonne `col` du tableau. */
export const lineTabIndex = (col: number, row: number) => (col + 1) * ROWS_PER_COLUMN + row + 1;

/** tabindex du n-ième champ des totaux. */
export const totalsTabIndex = (n: number) => TOTALS_BASE + n;

/** Colonne de rangement de chaque cellule d'une ligne, d'après les colspan (pleine largeur → Désignation). */
export function cellColumns(colSpans: number[]): number[] {
  let col = 0;
  return colSpans.map((span) => {
    const target = span > 1 ? WIDE_COLUMN : col;
    col += span;
    return target;
  });
}

const isField = (el: Element) =>
  (el instanceof HTMLInputElement && el.type !== "checkbox") || el instanceof HTMLTextAreaElement;

/**
 * Pose les tabindex des champs du tableau `table` (lignes du tbody) et du panneau `totals`.
 * À rappeler après chaque rendu (lignes ajoutées, déplacées, supprimées).
 */
export function applyColumnTabOrder(table: HTMLTableElement | null, totals: HTMLElement | null) {
  table?.tBodies[0]?.querySelectorAll("tr").forEach((tr, row) => {
    const cells = [...tr.cells];
    const columns = cellColumns(cells.map((td) => td.colSpan));
    cells.forEach((td, i) =>
      td.querySelectorAll("input, textarea").forEach((el) => {
        if (isField(el)) (el as HTMLElement).tabIndex = lineTabIndex(columns[i], row);
      }),
    );
  });
  let n = 0;
  totals?.querySelectorAll("input, textarea").forEach((el) => {
    if (isField(el)) (el as HTMLElement).tabIndex = totalsTabIndex(n++);
  });
}
