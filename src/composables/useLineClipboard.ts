import { ref } from "vue";
import type { QuoteLine } from "../api";

interface LineClipboard {
  lines: QuoteLine[];
  /** Conditions de prix du devis d'origine : si elles diffèrent à l'arrivée, les prix sont recalculés. */
  pricingKey: string;
  /** Version texte (tabulée) mise dans le presse-papiers système. */
  text: string;
}

const frNumber = (v: number | null) => (v == null ? "" : String(v).replace(".", ","));

/** Texte tabulé pour coller dans Excel ou un mail. */
export function linesToText(lines: QuoteLine[]) {
  return lines
    .map((l) =>
      l.kind !== "item"
        ? l.designation
        : [l.product_ref, l.designation, frNumber(l.quantity), frNumber(l.unit_price), l.discount ? `${frNumber(l.discount)} %` : ""].join("\t"),
    )
    .join("\n");
}

const normalize = (t: string) => t.replace(/\r\n/g, "\n").trim();

// Partagé entre les devis : on copie dans l'un, on colle dans l'autre.
const clipboard = ref<LineClipboard | null>(null);

export function useLineClipboard() {
  function copy(lines: QuoteLine[], pricingKey: string): string {
    const text = linesToText(lines);
    clipboard.value = {
      lines: lines.map(
        ({ kind, product_ref, enedis_code, designation, quantity, unit_price, discount, is_option, price_source, public_price, threshold_price, discounted_price, lpn_price, lpn_list }) => ({
          kind,
          product_ref,
          enedis_code,
          designation,
          quantity,
          unit_price,
          discount,
          is_option,
          price_source,
          public_price,
          threshold_price,
          discounted_price,
          lpn_price,
          lpn_list,
        }),
      ),
      pricingKey,
      text,
    };
    return text;
  }

  /** Le presse-papiers système contient-il encore nos lignes (et pas un texte copié depuis) ? */
  const holdsLines = (systemText: string) => !!clipboard.value && normalize(systemText) === normalize(clipboard.value.text);

  return { clipboard, copy, holdsLines };
}
