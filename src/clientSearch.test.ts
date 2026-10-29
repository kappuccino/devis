import { describe, expect, it } from "vitest";
import { prefixFirst } from "./clientSearch";

describe("prefixFirst", () => {
  const clients = [
    { code: "001", name: "ELEC DU ROYANS" },
    { code: "002", name: "AUBERT LEROY" },
    { code: "003", name: "ROYER SARL" },
    { code: "ROY1", name: "DUPONT" },
    { code: "004", name: "ROY ÉLECTRICITÉ" },
  ];

  it("met d'abord les raisons sociales qui commencent par le texte, puis un mot ou le code", () => {
    expect(prefixFirst(clients, " roy").map((c) => c.code)).toEqual(["003", "004", "001", "ROY1", "002"]);
  });

  it("garde l'ordre sans recherche", () => {
    expect(prefixFirst(clients, "")).toBe(clients);
  });
});
