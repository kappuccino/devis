import pdfMake from "pdfmake/build/pdfmake";
import pdfFonts from "pdfmake/build/vfs_fonts";
import type { Content, TableCell, TDocumentDefinitions } from "pdfmake/interfaces";
import { save } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, type Client, type Quote, type Settings } from "../api";
import { formatDate, formatEuro, formatNumber, formatUnitPrice, round2 } from "../format";
import { lineTotal, subtotals } from "../quoteLines";

pdfMake.addVirtualFileSystem(pdfFonts);

/** Rouge du logo Cahors, et sa teinte claire pour les fonds. */
const ACCENT = "#e60005";
const ACCENT_TINT = "#fdeced";

export function buildDocument(quote: Quote, client: Client | null, s: Settings): TDocumentDefinitions {
  const company: Content[] = [
    { text: s.company_name || "Ma société", style: "companyName" },
    ...(s.company_address ? [{ text: s.company_address }] : []),
    ...[s.company_phone && `Tél. ${s.company_phone}`, s.company_email, s.company_siret && `SIRET ${s.company_siret}`]
      .filter(Boolean)
      .map((t) => ({ text: t as string, color: "#555" })),
  ];

  const header: Content = {
    columns: [
      s.company_logo
        ? { stack: [{ image: s.company_logo, fit: [140, 70], margin: [0, 0, 0, 6] }, ...company], width: "*" }
        : { stack: company, width: "*" },
      {
        width: 200,
        stack: [
          { text: "DEVIS", style: "title" },
          { text: `N° ${quote.number ?? "—"}`, bold: true, alignment: "right" },
          { text: `Date : ${formatDate(quote.date)}`, alignment: "right" },
          ...(s.quote_validity ? [{ text: `Validité : ${s.quote_validity}`, alignment: "right" as const }] : []),
        ],
      },
    ],
  };

  const clientBox: Content = {
    margin: [260, 24, 0, 24],
    table: {
      widths: ["*"],
      body: [
        [
          {
            stack: [
              // La raison sociale enregistrée dans le devis fait foi (client ponctuel ou nom ajusté).
              { text: quote.client_name, bold: true, fontSize: 11 },
              ...(quote.client_code ? [{ text: `Code client : ${quote.client_code}`, color: "#555" }] : []),
              ...(client?.siren ? [{ text: `SIREN : ${client.siren}`, color: "#555" }] : []),
              ...(client?.email ? [{ text: client.email.split(/[\s;,]+/)[0], color: "#555" }] : []),
            ],
            margin: [8, 6, 8, 6],
          },
        ],
      ],
    },
    layout: { hLineColor: () => "#ccc", vLineColor: () => "#ccc" },
  };

  const th = (text: string, alignment: "left" | "right" = "left") => ({ text, style: "th", alignment });
  // Lignes de texte sur toute la largeur ; sous-totaux surlignés.
  // La colonne « Remise » n'apparaît que si au moins une ligne a une remise supplémentaire.
  // Idem pour le code ENEDIS (avant la référence) : seulement si au moins un produit en a un.
  const withDiscount = quote.lines.some((l) => l.kind === "item" && l.discount > 0);
  const withEnedis = quote.lines.some((l) => l.kind === "item" && l.enedis_code);
  const cols = 6 + (withDiscount ? 1 : 0) + (withEnedis ? 1 : 0);
  const empties = (n: number) => Array.from({ length: n }, () => ({}));
  const body: TableCell[][] = [
    [
      ...(withEnedis ? [th("Code ENEDIS")] : []),
      th("Référence"),
      th("Désignation"),
      th("Qté", "right"),
      th("Prix public", "right"),
      th("PU HT", "right"),
      ...(withDiscount ? [th("Remise", "right")] : []),
      th("Total HT", "right"),
    ],
  ];
  const subtotalRows = new Set<number>();
  const amounts = subtotals(quote.lines);
  for (const l of quote.lines) {
    if (l.kind === "text") {
      body.push([{ text: l.designation, colSpan: cols, style: "textLine" }, ...empties(cols - 1)]);
    } else if (l.kind === "subtotal") {
      subtotalRows.add(body.length);
      body.push([
        { text: l.designation, colSpan: cols - 1, alignment: "right", bold: true },
        ...empties(cols - 2),
        { text: formatEuro(amounts.get(l)), alignment: "right", bold: true },
      ]);
    } else {
      body.push([
        ...(withEnedis ? [{ text: l.enedis_code ?? "", style: "mono" }] : []),
        { text: l.product_ref, style: "mono" },
        // Ligne en option : signalée, et hors total HT (comptée dans « Total options »).
        l.is_option ? { text: [l.designation, { text: "  (option)", style: "optionTag" }], italics: true } : l.designation,
        { text: formatNumber(l.quantity), alignment: "right" },
        { text: formatUnitPrice(l.public_price), alignment: "right", color: "#777" },
        { text: formatUnitPrice(l.unit_price), alignment: "right" },
        ...(withDiscount ? [{ text: l.discount ? `${formatNumber(l.discount)} %` : "", alignment: "right" as const }] : []),
        { text: formatEuro(lineTotal(l)), alignment: "right" },
      ]);
    }
  }

  const lines: Content = {
    table: {
      headerRows: 1,
      widths: [
        ...(withEnedis ? [54] : []),
        withEnedis ? 54 : 62,
        "*",
        34,
        withDiscount || withEnedis ? 56 : 62,
        withDiscount || withEnedis ? 56 : 62,
        ...(withDiscount ? [42] : []),
        withDiscount || withEnedis ? 64 : 70,
      ],
      body,
      dontBreakRows: true,
    },
    layout: {
      hLineWidth: (i, node) => (i === 0 || i === 1 || i === node.table.body.length ? 1 : 0.5),
      vLineWidth: () => 0,
      hLineColor: (i) => (i <= 1 ? ACCENT : "#ddd"),
      paddingTop: () => 5,
      paddingBottom: () => 5,
      fillColor: (row) => (row === 0 || subtotalRows.has(row) ? ACCENT_TINT : null),
    },
  };

  // TOTAL HT, puis remise globale et TOTAL HT remisé s'il y en a une.
  const totalRows: TableCell[][] = [
    [{ text: "TOTAL HT", bold: true }, { text: formatEuro(quote.total_ht), bold: true, alignment: "right" }],
  ];
  if (quote.discount_pct > 0) {
    totalRows.push(
      [
        { text: `Remise ${formatNumber(quote.discount_pct)} %` },
        { text: `− ${formatEuro(round2(quote.total_ht - quote.total_net))}`, alignment: "right" },
      ],
      [
        { text: "TOTAL HT REMISÉ", bold: true, color: ACCENT },
        { text: formatEuro(quote.total_net), bold: true, color: ACCENT, alignment: "right" },
      ],
    );
  }
  const total: Content = {
    margin: [0, 12, 0, 0],
    columns: [
      { width: "*", text: "" },
      {
        width: 220,
        table: { widths: ["*", "auto"], body: totalRows },
        layout: { hLineColor: () => ACCENT, vLineColor: () => ACCENT, paddingTop: () => 6, paddingBottom: () => 6 },
      },
    ],
  };

  // Lignes en option : leur total à part, tout en bas, hors total HT.
  const optionsTotal: Content | null =
    quote.total_options > 0
      ? {
          margin: [0, 10, 0, 0],
          columns: [
            { width: "*", text: "" },
            {
              width: 220,
              table: {
                widths: ["*", "auto"],
                body: [
                  [
                    { text: "TOTAL OPTIONS HT", italics: true },
                    { text: formatEuro(quote.total_options), italics: true, alignment: "right" },
                  ],
                ],
              },
              layout: { hLineColor: () => "#999", vLineColor: () => "#999", paddingTop: () => 6, paddingBottom: () => 6 },
            },
          ],
        }
      : null;

  const content: Content[] = [header, clientBox, lines, total, ...(optionsTotal ? [optionsTotal] : [])];
  if (quote.notes) content.push({ text: quote.notes, margin: [0, 24, 0, 0] });
  if (s.quote_conditions) content.push({ text: s.quote_conditions, style: "conditions" });

  return {
    pageSize: "A4",
    pageMargins: [40, 40, 40, 50],
    info: { title: `Devis ${quote.number ?? ""}` },
    content,
    footer: (page, pages) => ({
      columns: [
        { text: s.company_name ?? "", color: "#999" },
        { text: `${page} / ${pages}`, alignment: "right", color: "#999" },
      ],
      margin: [40, 16, 40, 0],
      fontSize: 8,
    }),
    defaultStyle: { fontSize: 9, lineHeight: 1.15 },
    styles: {
      companyName: { fontSize: 14, bold: true, color: ACCENT, margin: [0, 0, 0, 2] },
      title: { fontSize: 22, bold: true, color: ACCENT, alignment: "right", margin: [0, 0, 0, 4] },
      th: { bold: true, color: ACCENT },
      textLine: { italics: true, color: "#444" },
      optionTag: { color: ACCENT, fontSize: 8 },
      mono: { fontSize: 8.5 },
      conditions: { fontSize: 8, color: "#666", margin: [0, 24, 0, 0] },
    },
  };
}

/** Génère le PDF du devis, demande où l'enregistrer puis l'ouvre. Renvoie le chemin, ou null si annulé. */
export async function exportQuotePdf(quote: Quote): Promise<string | null> {
  const [client, settings] = await Promise.all([
    quote.client_code ? api.getClient(quote.client_code) : Promise.resolve(null),
    api.getSettings(),
  ]);
  const safeName = (quote.client_name || quote.client_code || "client").replace(/[\\/:*?"<>|]/g, "-");
  const path = await save({
    defaultPath: `${quote.number ?? "Devis"} - ${safeName}.pdf`,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!path) return null;

  const buffer = await pdfMake.createPdf(buildDocument(quote, client, settings)).getBuffer();
  await api.saveFile(path, new Uint8Array(buffer));
  await openPath(path).catch(() => {});
  return path;
}
