import { describe, expect, it } from "vitest";
import { autoFeeChanges, feeRulesFrom, ratesFor, shippingSuggestionRange } from "./fees";

describe("frais automatiques", () => {
  // Valeurs par défaut : sous 140 €, facturation 25 € et port 31 € ; port suggéré de 140 à 840 €.
  const rules = feeRulesFrom({});

  it("règles par défaut", () => {
    expect(rules).toEqual({ billing: { threshold: 140, amount: 25 }, shipping: { threshold: 140, amount: 31 } });
    expect(feeRulesFrom({ fee_shipping_amount: "35,5" }).shipping).toEqual({ threshold: 140, amount: 35.5 });
    // Champ laissé vide : valeur par défaut ; 0 : désactivé.
    expect(feeRulesFrom({ fee_billing_amount: "", fee_billing_threshold: "" }).billing).toEqual({ threshold: 140, amount: 25 });
    expect(feeRulesFrom({ fee_billing_amount: "0" }).billing).toBeUndefined();
    expect(feeRulesFrom({ fee_shipping_amount: "0" }).shipping).toBeUndefined();
    expect(shippingSuggestionRange({})).toEqual({ from: 140, to: 840 });
    expect(shippingSuggestionRange({ fee_shipping_threshold: "0" })).toBeNull();
  });

  it("ajoute facturation et port sous le minimum, les retire au-dessus", () => {
    expect(autoFeeChanges(100, rules, [], new Set())).toEqual({ add: ["shipping", "billing"], remove: [] });
    expect(
      autoFeeChanges(200, rules, [{ kind: "shipping", auto: true }, { kind: "billing", auto: true }], new Set()),
    ).toEqual({ add: [], remove: ["shipping", "billing"] });
  });

  it("ne touche pas à un frais modifié ou retiré à la main, ni à un devis vide", () => {
    expect(autoFeeChanges(200, rules, [{ kind: "billing", auto: false }], new Set())).toEqual({ add: [], remove: [] });
    expect(autoFeeChanges(100, rules, [], new Set(["billing"]))).toEqual({ add: ["shipping"], remove: [] });
    expect(autoFeeChanges(0, rules, [], new Set())).toEqual({ add: [], remove: [] });
  });
});

describe("tableau des frais de port", () => {
  const rates = [
    { department: "1", destination: "Ain", price: 53 },
    { department: "4", destination: "Alpes-de-Haute-Provence", price: 64 },
    { department: "4 VILLES HAUTES MONTAGNES", destination: "Alpes-de-Haute-Provence", price: 75 },
    { department: "6", destination: "Alpes-Maritimes", price: 60 },
    { department: "2A", destination: "Corse-du-Sud", price: 90 },
    { department: "2B", destination: "Haute-Corse", price: 92 },
    { department: "972", destination: "Martinique", price: 150 },
  ];
  const deps = (typed: string) => ratesFor(rates, typed).map((r) => r.department);

  it("par numéro de département, variantes comprises", () => {
    expect(deps("4")).toEqual(["4", "4 VILLES HAUTES MONTAGNES"]);
    expect(deps("04")).toEqual(["4", "4 VILLES HAUTES MONTAGNES"]);
    expect(deps("1")).toEqual(["1"]);
    expect(deps("2a")).toEqual(["2A"]);
    expect(ratesFor([{ department: "20", destination: "Corse", price: 97 }], "2B").length).toBe(1);
    expect(deps("99")).toEqual([]);
  });

  it("par code postal ou par nom", () => {
    expect(deps("01000")).toEqual(["1"]);
    expect(deps("97200")).toEqual(["972"]);
    expect(deps("20000")).toEqual(["2A", "2B"]);
    expect(deps("alpes mar")).toEqual(["6"]);
    expect(deps("")).toEqual([]);
  });
});
