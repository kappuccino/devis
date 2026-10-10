import pdfMake from "pdfmake/build/pdfmake";
import pdfFonts from "pdfmake/build/vfs_fonts";
import type { Content, TableCell, TDocumentDefinitions } from "pdfmake/interfaces";
import { openFile, pickSavePath } from "../dialogs";
import { api, type Client, type Quote, type Settings } from "../api";
import { formatDate, formatEuro, formatNumber, formatUnitPrice, round2 } from "../format";
import { isFee, lineTotal, quoteTotals, subtotals } from "../quoteLines";
import { conditionsText, parseConditions } from "../conditions";
import { PDFDocument } from "pdf-lib";
import { readFile } from "../docs/files";

pdfMake.addVirtualFileSystem(pdfFonts);

/** Rouge du logo Cahors, et sa teinte claire pour les fonds. */
const ACCENT = "#e60005";
const ACCENT_TINT = "#fdeced";

/** Siège : imprimé tel quel sur tous les devis, sous le nom de la société. */
export const HEAD_OFFICE = [
  "CAHORS – 372 av Pierre Bourrieres – 46003 CAHORS – Tél. 05 65 35 72 11",
  "ENVOI DES COMMANDES à maec-commande@groupe-cahors.com",
];

/** Réglage : coordonnées de l'agence (texte libre, imprimé sous le siège). */
export const AGENCY_KEY = "agency_contact";

/**
 * Coordonnées de l'agence. Tant qu'elles n'ont pas été saisies, reprises des anciens champs
 * séparés (adresse, téléphone, email, SIRET).
 */
export function agencyContact(s: Settings): string {
  if (AGENCY_KEY in s) return s[AGENCY_KEY];
  return [s.company_address, s.company_phone && `Tél. ${s.company_phone}`, s.company_email, s.company_siret && `SIRET ${s.company_siret}`]
    .filter(Boolean)
    .join("\n");
}

const MUTED = "#6b7280";
const TEXT = "#1f2328";
const BORDER = "#e3e6ea";
/** Gris clair des pastilles de sous-total et du bandeau TOTAL HT. */
const GREY_FILL = "#eeeff1";
/** Largeur utile de la page A4 (595 pt − marges de 40 pt). */
const PAGE_WIDTH = 515;

/** Petit titre de section en capitales rouges (CLIENT, CONDITIONS…). */
const sectionLabel = (text: string, margin: [number, number, number, number] = [0, 0, 0, 3]): Content => ({
  text,
  style: "sectionLabel",
  margin,
});

/** Encadré léger (bordure grise fine) autour d'un contenu. */
const softBox = (content: Content, padding = 8): Content => ({
  table: { widths: ["*"], body: [[content]] },
  layout: {
    hLineColor: () => BORDER,
    vLineColor: () => BORDER,
    hLineWidth: () => 0.75,
    vLineWidth: () => 0.75,
    paddingLeft: () => padding,
    paddingRight: () => padding,
    paddingTop: () => padding,
    paddingBottom: () => padding,
  },
});

/**
 * Devis PDF : en-tête (société à gauche ; DEVIS, numéro, date, validité, commercial à droite),
 * bloc client, tableau des lignes (en-tête plein, titres en bandeau, sous-totaux en pastille),
 * puis conditions / notes à gauche et totaux, TOTAL HT et « Bon pour accord » à droite.
 */
export function buildDocument(quote: Quote, client: Client | null, s: Settings): TDocumentDefinitions {
  const agency = agencyContact(s).trim();
  const company: Content[] = [
    // Avec un logo, le nom de la société ferait doublon : seulement sans logo.
    ...(s.company_logo ? [] : [{ text: s.company_name || "Ma société", style: "companyName" }]),
    { text: HEAD_OFFICE[0], color: MUTED },
    { text: HEAD_OFFICE[1], bold: true },
    ...(agency ? [{ text: agency, margin: [0, 6, 0, 0] as [number, number, number, number] }] : []),
  ];

  // Date, validité, commercial : libellés à gauche, valeurs à droite, sans cadre.
  const meta: [string, string][] = [
    ["En date du :", formatDate(quote.date)],
    ...(s.quote_validity ? [["Validité :", s.quote_validity] as [string, string]] : []),
    ...(quote.sales_rep ? [["Commercial :", quote.sales_rep] as [string, string]] : []),
  ];
  const header: Content = {
    columns: [
      s.company_logo
        ? { stack: [{ image: s.company_logo, fit: [140, 60], margin: [0, 0, 0, 8] }, ...company], width: "*" }
        : { stack: company, width: "*" },
      {
        width: 190,
        stack: [
          { text: "DEVIS", style: "title" },
          { text: `n° ${quote.number ?? "—"}`, color: ACCENT, alignment: "right", margin: [0, 0, 0, 8] },
          {
            columns: [
              { width: "*", text: "" },
              {
                width: "auto",
                table: { body: meta.map(([k, v]) => [{ text: k, color: MUTED }, { text: v, alignment: "right" }]) },
                layout: { defaultBorder: false, paddingLeft: () => 0, paddingRight: (i: number) => (i === 0 ? 10 : 0), paddingTop: () => 1, paddingBottom: () => 1 },
              },
            ],
          },
        ],
      },
    ],
  };

  // Client à droite ; nom de l'affaire à gauche, aligné sur le bas du bloc client (juste au-dessus
  // des lignes). Sans titres ni cadre : un tableau sans bordure permet l'alignement en bas.
  const project = quote.project_name?.trim();
  const clientBlock: Content = {
    margin: [0, 18, 0, 18],
    table: {
      widths: ["*", 250],
      body: [
        [
          project
            ? { text: project, bold: true, fontSize: 11, verticalAlignment: "bottom", margin: [0, 0, 15, 0] }
            : { text: "" },
          {
            stack: [
              // La raison sociale enregistrée dans le devis fait foi (client ponctuel ou nom ajusté).
              { text: quote.client_name, bold: true, fontSize: 9.5 },
              ...(quote.client_code ? [{ text: `Code client : ${quote.client_code}`, color: MUTED }] : []),
              ...(client?.siren ? [{ text: `SIREN : ${client.siren}`, color: MUTED }] : []),
              // Contact du devis ; à défaut, l'email général du client.
              ...(quote.contact_name ? [{ text: `À l'attention de ${quote.contact_name}`, margin: [0, 4, 0, 0] as [number, number, number, number] }] : []),
              ...[quote.contact_email || (!quote.contact_name && client?.email ? client.email.split(/[\s;,]+/)[0] : ""), quote.contact_phone && `Tél. ${quote.contact_phone}`]
                .filter(Boolean)
                .map((t) => ({ text: t as string, color: MUTED })),
            ],
          },
        ],
      ],
    },
    layout: {
      hLineWidth: () => 0,
      vLineWidth: () => 0,
      paddingLeft: () => 0,
      paddingRight: () => 0,
      paddingTop: () => 0,
      paddingBottom: () => 0,
    },
  } as Content;

  const th = (text: string, alignment: "left" | "right" = "left") => ({ text, style: "th", alignment });
  // La colonne « Remise » n'apparaît que si au moins une ligne a une remise supplémentaire.
  // Idem pour le code ENEDIS (avant la référence) : seulement si au moins un produit en a un.
  const withDiscount = quote.lines.some((l) => l.kind === "item" && l.discount > 0);
  const withEnedis = quote.lines.some((l) => l.kind === "item" && l.enedis_code);
  // Le prix public n'est jamais imprimé (le client ne voit que son prix net).
  const cols = 5 + (withDiscount ? 1 : 0) + (withEnedis ? 1 : 0);
  const empties = (n: number) => Array.from({ length: n }, () => ({}));
  const body: TableCell[][] = [
    [
      ...(withEnedis ? [th("Code ENEDIS")] : []),
      th("Référence"),
      th("Désignation"),
      th("Qté", "right"),
      th("PU HT", "right"),
      ...(withDiscount ? [th("Remise", "right")] : []),
      th("Total HT", "right"),
    ],
  ];
  const titleRows = new Set<number>();
  const amounts = subtotals(quote.lines);
  for (const l of quote.lines) {
    if (l.kind === "title") {
      // Titre de paragraphe : bandeau rouge pâle sur toute la largeur.
      titleRows.add(body.length);
      body.push([{ text: l.designation, colSpan: cols, style: "titleLine" }, ...empties(cols - 1)]);
    } else if (l.kind === "text") {
      body.push([{ text: l.designation, colSpan: cols, style: "textLine" }, ...empties(cols - 1)]);
    } else if (isFee(l.kind)) {
      // Frais de port / de facturation : dans le bloc des totaux, après la remise.
      continue;
    } else if (l.kind === "subtotal") {
      // Sous-total : pastille gris clair à droite (libellé : montant).
      // Sur les dernières colonnes (jusqu'à 4, en laissant au moins Référence et Désignation) :
      // un libellé long tient sur une ligne.
      const span = Math.min(4, cols - 2);
      body.push([
        { text: "", colSpan: cols - span },
        ...empties(cols - span - 1),
        {
          text: [{ text: `${l.designation} : ` }, { text: formatEuro(amounts.get(l)), bold: true }],
          colSpan: span,
          alignment: "right",
          fillColor: GREY_FILL,
          margin: [0, 1, 0, 1],
        },
        ...empties(span - 1),
      ]);
    } else {
      body.push([
        ...(withEnedis ? [{ text: l.enedis_code ?? "", style: "mono" }] : []),
        { text: l.product_ref, style: "mono" },
        // Ligne en option : signalée, et hors total HT (comptée dans « Total options »).
        l.is_option ? { text: [l.designation, { text: "  (option)", style: "optionTag" }], italics: true } : l.designation,
        { text: formatNumber(l.quantity), alignment: "right" },
        { text: formatUnitPrice(l.unit_price), alignment: "right" },
        ...(withDiscount ? [{ text: l.discount ? `${formatNumber(l.discount)} %` : "", alignment: "right" as const }] : []),
        { text: formatEuro(lineTotal(l)), alignment: "right" },
      ]);
    }
  }

  const lines: Content = {
    table: {
      headerRows: 1,
      widths: [...(withEnedis ? [54] : []), withEnedis ? 54 : 62, "*", 38, 62, ...(withDiscount ? [46] : []), 70],
      body,
      dontBreakRows: true,
    },
    layout: {
      // Pas de quadrillage : en-tête plein, bandeaux de titre, simple filet sous le tableau.
      hLineWidth: (i, node) => (i === node.table.body.length ? 0.75 : 0),
      hLineColor: () => BORDER,
      // Séparations verticales de la couleur du fond : sans elles, l'en-tête plein montre de
      // fins traits blancs entre les cellules.
      vLineWidth: () => 1,
      vLineColor: (_i, _node, row) => (row === 0 ? ACCENT_TINT : "#fff"),
      paddingTop: (i) => (i === 0 ? 5 : titleRows.has(i) ? 4 : 3),
      paddingBottom: (i) => (i === 0 ? 5 : titleRows.has(i) ? 4 : 3),
      fillColor: (row) => (row === 0 || titleRows.has(row) ? ACCENT_TINT : null),
    },
  };

  // Totaux, dans l'ordre du calcul : produits, remise globale (sur les produits seulement),
  // frais (jamais remisés), puis TOTAL HT en bandeau gris. Les frais au-dessus de la remise
  // laissaient croire qu'elle portait aussi sur eux.
  const fees = quote.lines.filter((l) => isFee(l.kind));
  const totals = quoteTotals(quote.lines, quote.discount_pct);
  const row = (label: string, amount: string, style: Record<string, unknown> = {}): TableCell[] => [
    { text: label, color: MUTED, ...style },
    { text: amount, alignment: "right", noWrap: true, ...style },
  ];
  const strong = { bold: true, color: TEXT };
  const details: TableCell[][] = [];
  if (quote.discount_pct > 0 || fees.length) details.push(row("Total produits HT", formatEuro(totals.products)));
  if (quote.discount_pct > 0) {
    details.push(
      row(`Remise ${formatNumber(quote.discount_pct)} % sur les produits`, `− ${formatEuro(totals.discount)}`),
      row("Total produits remisé HT", formatEuro(round2(totals.products - totals.discount)), strong),
    );
  }
  for (const f of fees) details.push(row(f.designation, formatEuro(lineTotal(f))));
  // Bandeau TOTAL HT gris clair : une seule cellule (deux cellules remplies laissent une couture).
  const band: TableCell[] = [
    {
      colSpan: 2,
      fillColor: GREY_FILL,
      margin: [0, 2, 0, 2],
      columns: [
        { text: "TOTAL HT", bold: true, fontSize: 10 },
        { text: formatEuro(totals.net), bold: true, fontSize: 10, alignment: "right", width: "auto", noWrap: true },
      ],
    },
    {},
  ];
  const totalsTable: Content = {
    table: { widths: ["*", "auto"], body: [...details, band] },
    layout: {
      // Cadre léger autour du détail, filets fins entre les lignes ; le bandeau ferme le bloc.
      hLineWidth: (i) => (i < details.length ? 0.75 : 0),
      vLineWidth: () => 0,
      hLineColor: () => BORDER,
      paddingLeft: () => 8,
      paddingRight: () => 8,
      paddingTop: (i) => (i === details.length ? 5 : 3),
      paddingBottom: (i) => (i === details.length ? 5 : 3),
    },
  };
  const right: Content[] = [totalsTable];
  // Lignes en option : leur total à part, hors total HT.
  if (quote.total_options > 0) {
    right.push({
      columns: [
        { text: "Total options HT (non compris)", italics: true, color: MUTED },
        { text: formatEuro(quote.total_options), italics: true, color: MUTED, alignment: "right", width: "auto" },
      ],
      margin: [8, 5, 8, 0],
    });
  }
  // Accord du client.
  right.push({
    margin: [0, 12, 0, 0],
    ...(softBox({ stack: [{ text: "Mention « Bon pour accord », date et signature", color: MUTED }, { text: " ", margin: [0, 0, 0, 48] }] }) as object),
  } as Content);

  // À gauche : notes puis conditions de vente, dans un encadré léger.
  const left: Content[] = [];
  if (quote.notes) left.push(sectionLabel("NOTES"), { text: quote.notes, margin: [0, 0, 0, 12] });
  const conditions = conditionsText(s).trim();
  if (conditions) {
    left.push(
      sectionLabel("CONDITIONS"),
      softBox({
        style: "conditions",
        stack: parseConditions(conditions).map((line) => ({
          text: line.runs.length
            ? line.runs.map((r) => ({ text: r.text, bold: r.bold, decoration: r.underline ? ("underline" as const) : undefined }))
            : " ",
          ...(line.title ? { bold: true, fontSize: 8, color: TEXT, margin: [0, 0, 0, 3] as [number, number, number, number] } : {}),
        })),
      }),
    );
  }

  const bottom: Content = {
    margin: [0, 16, 0, 0],
    unbreakable: true,
    columns: [
      { width: "*", stack: left.length ? left : [{ text: "" }] },
      { width: 230, stack: right },
    ],
    columnGap: 20,
  };

  return {
    pageSize: "A4",
    pageMargins: [40, 40, 40, 60],
    info: { title: `Devis ${quote.number ?? ""}` },
    content: [header, clientBlock, lines, bottom],
    // Pied de page : filet rouge, siège à gauche, n° du devis et page à droite.
    footer: (page, pages) => ({
      margin: [40, 14, 40, 0],
      stack: [
        { canvas: [{ type: "line", x1: 0, y1: 0, x2: PAGE_WIDTH, y2: 0, lineWidth: 0.75, lineColor: ACCENT }] },
        {
          margin: [0, 5, 0, 0],
          columns: [
            { text: HEAD_OFFICE[0], color: MUTED },
            { text: `Devis n° ${quote.number ?? "—"} · Page ${page} / ${pages}`, alignment: "right", color: MUTED, width: "auto" },
          ],
        },
      ],
      fontSize: 6.5,
    }),
    defaultStyle: { fontSize: 8, lineHeight: 1.15, color: TEXT },
    styles: {
      companyName: { fontSize: 12, bold: true, color: ACCENT, margin: [0, 0, 0, 2] },
      title: { fontSize: 24, alignment: "right" },
      th: { bold: true, color: ACCENT, fontSize: 7.5 },
      sectionLabel: { fontSize: 7, bold: true, color: ACCENT, characterSpacing: 0.6 },
      // Titre : gras, rouge, taille 10 (bandeau) ; texte : noir, gras, taille 8,5 (corps : 8).
      titleLine: { bold: true, color: ACCENT, fontSize: 10 },
      textLine: { bold: true, color: "#000", fontSize: 8.5 },
      optionTag: { color: ACCENT, fontSize: 7 },
      mono: { fontSize: 7.5 },
      conditions: { fontSize: 7, color: "#555" },
    },
  };
}

/** PDF du devis seul. */
export async function quotePdfBytes(quote: Quote): Promise<Uint8Array> {
  const [client, settings] = await Promise.all([
    quote.client_code ? api.getClient(quote.client_code) : Promise.resolve(null),
    api.getSettings(),
  ]);
  const buffer = await pdfMake.createPdf(buildDocument(quote, client, settings)).getBuffer();
  return new Uint8Array(buffer);
}

/** Réglage : PDF des conditions générales de vente complètes, ajouté en dernière page. */
export const CGV_PDF_KEY = "cgv_pdf_path";

/**
 * Ajoute les CGV complètes (PDF choisi dans les réglages) en fin de document.
 * Fichier absent ou illisible : le PDF est produit sans elles, avec un avertissement.
 */
export async function appendCgv(bytes: Uint8Array): Promise<{ bytes: Uint8Array; warning?: string }> {
  const path = (await api.getSettings())[CGV_PDF_KEY];
  if (!path) return { bytes };
  try {
    const [doc, cgv] = await Promise.all([
      PDFDocument.load(bytes),
      readFile(path).then((b) => PDFDocument.load(b, { ignoreEncryption: true, updateMetadata: false })),
    ]);
    for (const page of await doc.copyPages(cgv, cgv.getPageIndices())) doc.addPage(page);
    return { bytes: await doc.save() };
  } catch (e) {
    return { bytes, warning: `CGV non ajoutées (${path}) : ${e instanceof Error ? e.message : String(e)}` };
  }
}

/** Demande où enregistrer le PDF d'un devis ; null si annulé. */
export function askQuotePdfPath(quote: Quote, suffix = "") {
  const safeName = (quote.client_name || quote.client_code || "client").replace(/[\\/:*?"<>|]/g, "-");
  return pickSavePath({
    defaultPath: `${quote.number ?? "Devis"} - ${safeName}${suffix}.pdf`,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
}

/** Enregistre le PDF puis l'ouvre. */
export async function writeAndOpenPdf(path: string, bytes: Uint8Array) {
  await api.saveFile(path, bytes);
  await openFile(path).catch(() => {});
}

/**
 * Génère le PDF du devis (CGV complètes en dernière page), demande où l'enregistrer puis l'ouvre.
 * Renvoie le chemin (null si annulé) et un éventuel avertissement.
 */
export async function exportQuotePdf(quote: Quote): Promise<{ path: string; warning?: string } | null> {
  const path = await askQuotePdfPath(quote);
  if (!path) return null;
  const { bytes, warning } = await appendCgv(await quotePdfBytes(quote));
  await writeAndOpenPdf(path, bytes);
  return { path, warning };
}

/** Page de transition entre le devis et la documentation : le titre seul, en gros, centré. */
export async function transitionPdfBytes(): Promise<Uint8Array> {
  const doc: TDocumentDefinitions = {
    pageSize: "A4",
    pageMargins: [56, 56, 56, 56],
    content: [
      {
        text: "Documentation technique relative aux produits du devis",
        fontSize: 26,
        bold: true,
        color: ACCENT,
        alignment: "center",
        margin: [24, 300, 24, 0],
      },
    ],
  };
  return new Uint8Array(await pdfMake.createPdf(doc).getBuffer());
}
