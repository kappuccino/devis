/**
 * Montant saisi dans le PU HT : un nombre (« 1 234,5 ») ou une formule comme sous Excel
 * (« =24,3*0,4 », « =24.3*(1-40%) »). Opérations : + − * / et parenthèses ; « % » divise par 100 ;
 * virgule ou point décimal. Vide, illisible ou incomplet (frappe en cours) → null.
 */
export function evalAmount(text: string): number | null {
  const src = text.replace(/[\s  €]/g, "").replace(/^=/, "").replace(/,/g, ".");
  if (!src) return null;
  let pos = 0;
  const peek = () => src[pos];

  // expr := term (('+' | '-') term)* ; term := factor (('*' | '/') factor)* ;
  // factor := ('+' | '-') factor | primary '%'* ; primary := nombre | '(' expr ')'
  function expr(): number {
    let v = term();
    while (peek() === "+" || peek() === "-") v = src[pos++] === "+" ? v + term() : v - term();
    return v;
  }
  function term(): number {
    let v = factor();
    while (peek() === "*" || peek() === "x" || peek() === "/") v = src[pos++] === "/" ? v / factor() : v * factor();
    return v;
  }
  function factor(): number {
    if (peek() === "-") return pos++, -factor();
    if (peek() === "+") return pos++, factor();
    let v: number;
    if (peek() === "(") {
      pos++;
      v = expr();
      if (src[pos++] !== ")") throw new Error();
    } else {
      const m = /^\d*\.?\d+|^\d+\./.exec(src.slice(pos));
      if (!m) throw new Error();
      pos += m[0].length;
      v = Number(m[0]);
    }
    while (peek() === "%") pos++, (v /= 100);
    return v;
  }

  try {
    const v = expr();
    return pos === src.length && Number.isFinite(v) ? v : null;
  } catch {
    return null;
  }
}

/** Texte saisi qui est une formule (commence par « = »). */
export const isFormula = (text: string) => text.trimStart().startsWith("=");
