import { describe, expect, it } from "vitest";
import { evalAmount } from "./formula";

describe("evalAmount", () => {
  it("lit un nombre", () => {
    expect(evalAmount("1 234,5 €")).toBe(1234.5);
    expect(evalAmount("-12")).toBe(-12);
    expect(evalAmount("")).toBeNull();
  });

  it("calcule une formule comme sous Excel", () => {
    expect(evalAmount("=24.3*0.4")).toBeCloseTo(9.72);
    expect(evalAmount("=24,3*0,4")).toBeCloseTo(9.72);
    expect(evalAmount("= 24,3 * (1 - 40%)")).toBeCloseTo(14.58);
    expect(evalAmount("=100-15%*100")).toBeCloseTo(85);
    expect(evalAmount("=2+3*4")).toBe(14);
    expect(evalAmount("=(2+3)*4")).toBe(20);
    expect(evalAmount("=10/4")).toBe(2.5);
    expect(evalAmount("=-5+2")).toBe(-3);
    expect(evalAmount("=3x2")).toBe(6);
  });

  it("refuse une formule incomplète ou illisible", () => {
    expect(evalAmount("=24.3*")).toBeNull();
    expect(evalAmount("=(2+3")).toBeNull();
    expect(evalAmount("=abc")).toBeNull();
    expect(evalAmount("=1/0")).toBeNull();
    expect(evalAmount("=")).toBeNull();
  });
});
