import { describe, expect, it } from "vitest";
import { conditionsText, DEFAULT_CONDITIONS, parseConditions } from "./conditions";

describe("conditions de vente", () => {
  it("titre, gras et souligné", () => {
    expect(parseConditions("# Titre\nNos prix sont **nets**.\n__Mention__ (ci-joint)\n")).toEqual([
      { title: true, runs: [{ text: "Titre" }] },
      { title: false, runs: [{ text: "Nos prix sont " }, { text: "nets", bold: true }, { text: "." }] },
      { title: false, runs: [{ text: "Mention", underline: true }, { text: " (ci-joint)" }] },
      { title: false, runs: [] },
    ]);
  });

  it("texte par défaut tant que les conditions n'ont jamais été renseignées", () => {
    expect(conditionsText({})).toBe(DEFAULT_CONDITIONS);
    expect(conditionsText({ quote_conditions: "" })).toBe("");
  });
});
