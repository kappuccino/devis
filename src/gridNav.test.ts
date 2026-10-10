import { describe, expect, it } from "vitest";
import { cellColumns, lineTabIndex, totalsTabIndex, WIDE_COLUMN } from "./gridNav";

describe("Tab par colonne", () => {
  it("range chaque cellule dans sa colonne ; les champs pleine largeur dans la Désignation", () => {
    // poignée, case, ENEDIS, réf, désignation, qté
    expect(cellColumns([1, 1, 1, 1, 1, 1])).toEqual([0, 1, 2, 3, 4, 5]);
    // titre : poignée, case, champ sur 11 colonnes, cellule vide
    expect(cellColumns([1, 1, 11, 1])).toEqual([0, 1, WIDE_COLUMN, 13]);
  });

  it("ordre : toute une colonne de haut en bas, puis la suivante, puis les totaux", () => {
    const order = [lineTabIndex(3, 0), lineTabIndex(3, 1), lineTabIndex(4, 0), lineTabIndex(4, 1), totalsTabIndex(0)];
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(lineTabIndex(3, 998)).toBeLessThan(lineTabIndex(4, 0));
  });
});
