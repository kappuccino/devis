import { describe, expect, it } from "vitest";
import { autoFeeChanges, feeRulesFrom } from "./fees";

describe("frais automatiques", () => {
  const rules = feeRulesFrom({}); // valeurs par défaut : facturation 25 € sous 140 € ; port désactivé (pas de montant)

  it("règles par défaut", () => {
    expect(rules).toEqual({ billing: { threshold: 140, amount: 25 } });
    expect(feeRulesFrom({ fee_shipping_amount: "35,5" }).shipping).toEqual({ threshold: 840, amount: 35.5 });
    // Champ laissé vide : valeur par défaut ; 0 : désactivé.
    expect(feeRulesFrom({ fee_billing_amount: "", fee_billing_threshold: "" }).billing).toEqual({ threshold: 140, amount: 25 });
    expect(feeRulesFrom({ fee_billing_amount: "0" }).billing).toBeUndefined();
  });

  it("ajoute les frais de facturation sous le minimum, les retire au-dessus", () => {
    expect(autoFeeChanges(100, rules, [], new Set())).toEqual({ add: ["billing"], remove: [] });
    expect(autoFeeChanges(200, rules, [{ kind: "billing", auto: true }], new Set())).toEqual({ add: [], remove: ["billing"] });
  });

  it("ne touche pas à un frais modifié ou retiré à la main, ni à un devis vide", () => {
    expect(autoFeeChanges(200, rules, [{ kind: "billing", auto: false }], new Set())).toEqual({ add: [], remove: [] });
    expect(autoFeeChanges(100, rules, [], new Set(["billing"]))).toEqual({ add: [], remove: [] });
    expect(autoFeeChanges(0, rules, [], new Set())).toEqual({ add: [], remove: [] });
  });
});
