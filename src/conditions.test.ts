import { describe, expect, it } from "vitest";
import { CONDITIONS, parseConditions } from "./conditions";

describe("conditions de vente", () => {
  it("titre, gras et souligné", () => {
    expect(parseConditions("# Titre\nNos prix sont **nets**.\n__Mention__ (ci-joint)\n")).toEqual([
      { title: true, runs: [{ text: "Titre" }] },
      { title: false, runs: [{ text: "Nos prix sont " }, { text: "nets", bold: true }, { text: "." }] },
      { title: false, runs: [{ text: "Mention", underline: true }, { text: " (ci-joint)" }] },
      { title: false, runs: [] },
    ]);
  });

  it("conditions fixes : première ligne en gras, mention d'acceptation à la fin", () => {
    const lines = parseConditions(CONDITIONS);
    expect(lines[0].runs).toEqual([{ text: "VIREMENT SUR FACTURE 30 JOURS FIN DE MOIS LE 10", bold: true }]);
    expect(lines[lines.length - 1].runs[0].text).toMatch(/^Toute commande entraîne/);
  });
});
