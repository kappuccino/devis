import pdfMake from "pdfmake/build/pdfmake";
import pdfFonts from "pdfmake/build/vfs_fonts";
import type { Content, TableCell, TDocumentDefinitions } from "pdfmake/interfaces";
import { openFile, pickSavePath } from "../dialogs";
import { api, type Client, type Quote, type Settings } from "../api";
import { formatDate, formatEuro, formatNumber, formatUnitPrice, round2 } from "../format";
import { isFee, lineTotal, quoteTotals, subtotals } from "../quoteLines";
import { CONDITIONS, parseConditions } from "../conditions";
import { PDFDocument } from "pdf-lib";
import { readFile } from "../docs/files";

pdfMake.addVirtualFileSystem(pdfFonts);

/** Rouge du logo Cahors, et sa teinte claire pour les fonds. */
const ACCENT = "#e60005";
const ACCENT_TINT = "#fdeced";
/** En-tête du tableau des lignes : gris clair, un peu plus soutenu que les titres. */
const HEADER_FILL = "#d3d6db";

/** Pied de page : identique sur tous les devis (siège et adresse des commandes), non modifiable. */
export const HEAD_OFFICE = [
  "CAHORS – 372 av Pierre Bourrieres – 46003 CAHORS – Tél. 05 65 35 72 11",
  "SIRET : 451 01 058 00017 – NAF/APE 27.12Z",
];

/** Mention fixe au-dessus des conditions générales de vente. */
export const ORDERS_EMAIL = "maec-commande@groupe-cahors.com";

/** Validité imprimée quand le réglage est vide. */
export const DEFAULT_VALIDITY = "90 jours";

const MUTED = "#6b7280";
const TEXT = "#1f2328";
const BORDER = "#e3e6ea";
/** Gris clair des pastilles de sous-total et du bandeau TOTAL HT. */
const GREY_FILL = "#eeeff1";
/** Largeur utile de la page A4 (595 pt − marges de 40 pt). */
const PAGE_WIDTH = 515;
/** Bas du devis : colonne des totaux à droite, conditions générales à gauche. */
const TOTALS_WIDTH = 210;
const BOTTOM_GAP = 16;
const CONDITIONS_WIDTH = PAGE_WIDTH - TOTALS_WIDTH - BOTTOM_GAP;

/** Petit titre de section en capitales rouges (NOTES, CONDITIONS GÉNÉRALES DE VENTE). */
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

/** Rayon des coins arrondis (cadre client, conditions générales de vente). */
const RADIUS = 3;
type Corner = "tl" | "tr" | "bl" | "br";
/**
 * Coin arrondi d'un fond de cellule : pdfmake ne remplit les cellules qu'en rectangle ; un petit
 * masque blanc (le coin moins le quart de cercle) posé sur l'angle l'arrondit. (x, y) : position
 * de l'angle par rapport au début du contenu de la cellule, sans effet sur la mise en page.
 */
function cornerMask(corner: Corner, x: number, y: number, r = RADIUS): Content {
  const sx = corner === "tr" || corner === "br" ? -1 : 1;
  const sy = corner === "bl" || corner === "br" ? -1 : 1;
  const e = 0.4; // déborde un peu vers l'extérieur : aucun liseré de fond ne dépasse
  const arc = Array.from({ length: 9 }, (_, k) => {
    const t = Math.PI * (1 + k / 16);
    return [r + r * Math.cos(t), r + r * Math.sin(t)];
  });
  const pts = [[-e, -e], [-e, r], ...arc, [r, -e]].map(([px, py]) => ({ x: px * sx, y: py * sy }));
  return { relativePosition: { x, y }, alignment: "left", canvas: [{ type: "polyline", points: pts, closePath: true, color: "#fff", lineWidth: 0 }] } as unknown as Content;
}
/**
 * Contenu d'une cellule remplie, coins arrondis : `width` = largeur totale du fond, `pad` = écarts
 * [gauche, haut, bas] entre le bord du fond et le contenu.
 */
function rounded(content: Content[], width: number, [left, top, bottom]: [number, number, number], corners: Corner[] = ["tl", "tr", "bl", "br"]): Content[] {
  const at = (c: Corner, x: number, y: number) => (corners.includes(c) ? [cornerMask(c, x, y)] : []);
  return [...at("tl", -left, -top), ...at("tr", width - left, -top), ...content, ...at("bl", -left, bottom), ...at("br", width - left, bottom)];
}

/** Hauteur de la page A4 et marge du bas (place du pied de page). */
const PAGE_HEIGHT = 842;
const BOTTOM_MARGIN = 64;
/**
 * Conditions générales de vente, en bas à gauche du devis (à côté des totaux) : encadré gris
 * très clair à filet rouge épais à gauche, titre en petites capitales rouges, paragraphes serrés ;
 * le dernier (« Toute commande entraîne… ») en italique.
 */
function conditionsBlock(): Content {
  const paragraphs: Content[][] = [[]];
  for (const line of parseConditions(CONDITIONS)) {
    if (!line.runs.length) {
      if (paragraphs[paragraphs.length - 1].length) paragraphs.push([]);
      continue;
    }
    paragraphs[paragraphs.length - 1].push({
      text: line.runs.map((r) => ({ text: r.text, bold: r.bold, color: r.bold ? TEXT : undefined, decoration: r.underline ? ("underline" as const) : undefined })),
      ...(line.title ? { bold: true, color: TEXT } : {}),
    });
  }
  const last = paragraphs.length > 1 ? paragraphs.pop()! : [];
  return {
    table: {
      widths: ["*"],
      body: [
        [
          {
            style: "conditions",
            // Coins arrondis à droite ; à gauche, le filet rouge.
            stack: rounded(
              [
                { text: "CONDITIONS GÉNÉRALES DE VENTE", style: "sectionLabel", margin: [0, 0, 0, 4] },
                ...paragraphs.map((p, i) => ({ stack: p, margin: [0, i ? 3 : 0, 0, 0] })),
                ...(last.length ? [{ stack: last, italics: true, margin: [0, 4, 0, 0] }] : []),
              ] as Content[],
              CONDITIONS_WIDTH,
              [2.5 + 8, 6, 6],
              ["tr", "br"],
            ),
          },
        ],
      ],
    },
    layout: {
      hLineWidth: () => 0,
      vLineWidth: (i: number) => (i === 0 ? 2.5 : 0),
      vLineColor: () => ACCENT,
      fillColor: () => "#f6f7f8",
      paddingLeft: () => 8,
      paddingRight: () => 8,
      paddingTop: () => 6,
      paddingBottom: () => 6,
    },
  } as unknown as Content;
}

/** Pied de page : logo | siège et commandes | n° du devis et pastille de page. */
function footer(s: Settings, quote: Quote, page: number, pages: number): Content {
  const brand = (
    s.company_logo
      ? { image: s.company_logo, fit: [62, 16], width: 62 }
      : { text: s.company_name || "CAHORS", bold: true, color: ACCENT, fontSize: 9, width: "auto" }
  ) as Content;
  return {
    margin: [40, 16, 40, 0],
    fontSize: 6.5,
    columns: [
      brand,
      // Séparateur vertical fin entre le logo et l'adresse.
      { width: 1, canvas: [{ type: "line", x1: 0, y1: 0, x2: 0, y2: 18, lineWidth: 0.5, lineColor: BORDER }] },
      {
        width: "*",
        stack: [
          { text: HEAD_OFFICE[0], color: MUTED },
          { text: HEAD_OFFICE[1], color: MUTED },
        ],
        margin: [0, 1, 0, 0],
      },
      {
        width: "auto",
        stack: [
          { text: `Devis n° ${quote.number ?? "—"}`, color: MUTED, alignment: "right" },
          {
            alignment: "right",
            margin: [0, 2, 0, 0],
            table: { body: [[{ text: `${page} / ${pages}`, bold: true, color: "#fff", fillColor: ACCENT, fontSize: 7 }]] },
            layout: { defaultBorder: false, paddingLeft: () => 6, paddingRight: () => 6, paddingTop: () => 1, paddingBottom: () => 1 },
          },
        ],
      },
    ],
    columnGap: 10,
  } as Content;
}

/**
 * Devis PDF : en-tête (société à gauche ; DEVIS, numéro, date, validité, commercial à droite),
 * bloc client, tableau des lignes (en-tête plein, titres en bandeau, sous-totaux en pastille),
 * puis conditions / notes à gauche et totaux, TOTAL HT et « Bon pour accord » à droite.
 */
export function buildDocument(quote: Quote, client: Client | null, s: Settings): TDocumentDefinitions {

  // Date, validité, commercial : libellés à gauche, valeurs à droite, sans cadre.
  const meta: [string, string][] = [
    ["En date du :", formatDate(quote.date)],
    ["Validité :", s.quote_validity?.trim() || DEFAULT_VALIDITY],
    ...(quote.sales_rep ? [["Commercial :", quote.sales_rep] as [string, string]] : []),
  ];
  // Rédacteur du devis (Réglages) : sous le logo, en petit ; le client est à part, dans un cadre gris.
  const author: Content[] = [
    ...(s.author_name?.trim() ? [{ text: s.author_name.trim(), bold: true }] : []),
    ...[
      ...(s.author_address ?? "").split("\n"),
      s.author_phone?.trim() && `Tél. ${s.author_phone.trim()}`,
      s.author_email,
    ]
      .map((l) => (l ?? "").trim())
      .filter(Boolean)
      .map((t) => ({ text: t, color: MUTED })),
  ];

  // Coordonnées du client : à droite, dans un cadre gris (en face du nom de l'affaire).
  const clientDetails: Content[] = [
    // La raison sociale enregistrée dans le devis fait foi (client ponctuel ou nom ajusté).
    { text: quote.client_name, bold: true, fontSize: 9.5 },
    ...(quote.client_code ? [{ text: `Code client : ${quote.client_code}`, color: MUTED }] : []),
    ...(client?.siren ? [{ text: `SIREN : ${client.siren}`, color: MUTED }] : []),
    // Contact du devis ; à défaut, l'email général du client.
    ...(quote.contact_name ? [{ text: `À l'attention de ${quote.contact_name}`, margin: [0, 4, 0, 0] as [number, number, number, number] }] : []),
    ...[quote.contact_email || (!quote.contact_name && client?.email ? client.email.split(/[\s;,]+/)[0] : ""), quote.contact_phone && `Tél. ${quote.contact_phone}`]
      .filter(Boolean)
      .map((t) => ({ text: t as string, color: MUTED })),
  ];

  const header: Content = {
    columns: [
      {
        width: "*",
        stack: [
          // Avec un logo, le nom de la société ferait doublon : seulement sans logo.
          s.company_logo
            ? { image: s.company_logo, fit: [210, 72] }
            : { text: s.company_name || "Ma société", style: "companyName" },
          ...(author.length ? [{ stack: author, margin: [0, 8, 0, 0] as [number, number, number, number] }] : []),
        ],
      },
      {
        width: 190,
        stack: [
          { text: "DEVIS", style: "title" },
          { text: `n° ${quote.number ?? "—"}`, color: ACCENT, fontSize: 12, bold: true, alignment: "right", margin: [0, 0, 0, 6] },
          // Date, validité, commercial : en petit.
          {
            columns: [
              { width: "*", text: "" },
              {
                width: "auto",
                fontSize: 7,
                table: { body: meta.map(([k, v]) => [{ text: k, color: MUTED }, { text: v, alignment: "right" }]) },
                layout: { defaultBorder: false, paddingLeft: () => 0, paddingRight: (i: number) => (i === 0 ? 8 : 0), paddingTop: () => 0.5, paddingBottom: () => 0.5 },
              },
            ],
          },
        ],
      },
    ],
  };

  // Sous l'en-tête : le nom de l'affaire à gauche, mis en valeur (filet rouge, grand titre),
  // en face des coordonnées du client, à droite dans un cadre gris.
  const project = quote.project_name?.trim();
  const projectCell: TableCell = project
    ? {
        verticalAlignment: "middle",
        table: {
          widths: ["*"],
          body: [[{ text: project, fontSize: 14, color: TEXT, lineHeight: 1.05 }]],
        },
        layout: {
          hLineWidth: () => 0,
          vLineWidth: (i: number) => (i === 0 ? 3 : 0),
          vLineColor: () => ACCENT,
          paddingLeft: () => 10,
          paddingRight: () => 0,
          paddingTop: () => 3,
          paddingBottom: () => 3,
        },
      } as TableCell
    : { text: "" };
  const CLIENT_WIDTH = 250;
  const projectBlock: Content = {
    margin: [0, 14, 0, 16],
    table: {
      // Colonne vide de 20 pt entre les deux (le fond gris ne doit couvrir que le client).
      widths: ["*", 20, CLIENT_WIDTH],
      body: [
        [
          projectCell,
          { text: "" },
          // Cadre gris à coins arrondis, de la hauteur des coordonnées du client.
          {
            table: { widths: ["*"], body: [[{ stack: rounded(clientDetails, CLIENT_WIDTH, [6, 4, 4]), fillColor: GREY_FILL }]] },
            layout: { hLineWidth: () => 0, vLineWidth: () => 0, paddingLeft: () => 6, paddingRight: () => 6, paddingTop: () => 4, paddingBottom: () => 4 },
          },
        ],
      ],
    },
    layout: { defaultBorder: false, paddingLeft: () => 0, paddingRight: () => 0, paddingTop: () => 0, paddingBottom: () => 0 },
  };

  const th = (text: string, alignment: "left" | "right" = "left") => ({ text, style: "th", alignment });
  // Colonne « Remise » (remise supplémentaire de la ligne) : toujours présente.
  // Le code ENEDIS (avant la référence) : seulement si au moins un produit en a un.
  const withEnedis = quote.lines.some((l) => l.kind === "item" && l.enedis_code);
  // Prix public, avant le PU HT retenu : seulement si au moins un produit en a un.
  // Le prix seuil n'est jamais imprimé.
  const withPublic = quote.lines.some((l) => l.kind === "item" && l.public_price != null);
  const cols = 6 + (withEnedis ? 1 : 0) + (withPublic ? 1 : 0);
  const price = (v: number | null | undefined, style: Record<string, unknown> = {}) => ({
    text: v != null ? formatUnitPrice(v) : "",
    alignment: "right" as const,
    ...style,
  });
  // Largeurs sans les marges des cellules (3 pt de chaque côté) : le tout tient dans la page.
  const widths: (number | "*")[] = [
    ...(withEnedis ? [36] : []),
    withEnedis ? 44 : 52,
    "*",
    20,
    ...(withPublic ? [44] : []),
    withPublic ? 44 : 52,
    30,
    48,
  ];
  const empties = (n: number) => Array.from({ length: n }, () => ({}));
  const body: TableCell[][] = [
    [
      ...(withEnedis ? [th("Codet")] : []),
      th("Référence"),
      th("Désignation"),
      th("Qté", "right"),
      ...(withPublic ? [th("Prix public", "right")] : []),
      th("PU HT", "right"),
      th("Remise", "right"),
      th("Total HT", "right"),
    ],
  ];
  const titleRows = new Set<number>();
  const amounts = subtotals(quote.lines);
  for (const l of quote.lines) {
    if (l.kind === "title") {
      // Titre de paragraphe : bandeau gris clair sur toute la largeur.
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
        ...(withPublic ? [price(l.public_price, { color: MUTED })] : []),
        price(l.unit_price, withPublic ? { bold: true } : {}),
        { text: l.discount ? `${formatNumber(l.discount)} %` : "", alignment: "right" },
        { text: formatEuro(lineTotal(l)), alignment: "right" },
      ]);
    }
  }

  // Lignes en 7 pt : la plupart des désignations tiennent sur une seule ligne.
  const lines: Content = {
    fontSize: 7,
    table: {
      headerRows: 1,
      widths,
      body,
      dontBreakRows: true,
    },
    layout: {
      // Pas de quadrillage : en-tête plein, bandeaux de titre, simple filet sous le tableau.
      // Filet gris fin sous la dernière ligne (fin du devis, avant les totaux) ; un titre en
      // première ligne est décollé de l'en-tête par une bande blanche.
      hLineWidth: (i, node) => (i === node.table.body.length ? 0.8 : i === 1 && titleRows.has(1) ? 5 : 0),
      hLineColor: (i, node) => (i === node.table.body.length ? "#a9aeb5" : "#fff"),
      // Séparations verticales de la couleur du fond : sans elles, l'en-tête plein montre de
      // fins traits blancs entre les cellules.
      vLineWidth: () => 1,
      vLineColor: (_i, _node, row) => (row === 0 ? HEADER_FILL : "#fff"),
      paddingLeft: () => 3,
      paddingRight: () => 3,
      paddingTop: (i) => (i === 0 ? 2.5 : titleRows.has(i) ? 2 : 3),
      paddingBottom: (i) => (i === 0 ? 2 : titleRows.has(i) ? 1.5 : 3),
      fillColor: (row) => (row === 0 ? HEADER_FILL : titleRows.has(row) ? GREY_FILL : null),
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
      margin: [0, 1, 0, 1],
      columns: [
        { text: "TOTAL HT", bold: true, fontSize: 9 },
        { text: formatEuro(totals.net), bold: true, fontSize: 9, alignment: "right", width: "auto", noWrap: true },
      ],
    },
    {},
  ];
  // Bloc compact (7,5 pt) : un devis de quelques lignes tient sur une page.
  const totalsTable: Content = {
    fontSize: 7.5,
    table: { widths: ["*", "auto"], body: [...details, band] },
    layout: {
      // Cadre léger autour du détail, filets fins entre les lignes ; le bandeau ferme le bloc.
      hLineWidth: (i) => (i < details.length ? 0.75 : 0),
      vLineWidth: () => 0,
      hLineColor: () => BORDER,
      paddingLeft: () => 6,
      paddingRight: () => 6,
      paddingTop: (i) => (i === details.length ? 4 : 2),
      paddingBottom: (i) => (i === details.length ? 4 : 2),
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
      margin: [6, 4, 6, 0],
      fontSize: 7.5,
    });
  }
  // Accord du client.
  right.push({
    margin: [0, 8, 0, 0],
    ...(softBox({ stack: [{ text: "Mention « Bon pour accord », date et signature", color: MUTED, fontSize: 7.5 }, { text: " ", margin: [0, 0, 0, 34] }] }, 6) as object),
  } as Content);

  // À gauche : notes, envoi des commandes puis conditions générales de vente ; à droite : totaux et « Bon pour accord ».
  const left: Content[] = [
    ...(quote.notes ? [sectionLabel("NOTES"), { text: quote.notes, margin: [0, 0, 0, 8] as [number, number, number, number] }] : []),
    {
      text: [{ text: "Envoi des commandes : ", bold: true }, { text: ORDERS_EMAIL, bold: true, color: ACCENT }, "\nJoindre le devis à la commande."],
      fontSize: 7.5,
      color: TEXT,
      margin: [0, 0, 0, 10],
    },
    conditionsBlock(),
  ];

  const bottom: Content = {
    margin: [0, 12, 0, 0],
    unbreakable: true,
    columns: [
      { width: "*", stack: left },
      { width: TOTALS_WIDTH, stack: right },
    ],
    columnGap: BOTTOM_GAP,
  };

  return {
    pageSize: "A4",
    pageMargins: [40, 40, 40, BOTTOM_MARGIN],
    info: { title: `Devis ${quote.number ?? ""}` },
    content: [header, projectBlock, lines, bottom],
    // Pied de page : logo, siège et adresse des commandes, n° du devis et page en pastille rouge ;
    // un liseré rouge borde le haut et le bas de chaque page.
    footer: (page, pages) => footer(s, quote, page, pages),
    background: () => ({
      absolutePosition: { x: 0, y: 0 },
      canvas: [
        { type: "rect", x: 0, y: 0, w: PAGE_WIDTH + 80, h: 4, color: ACCENT },
        { type: "rect", x: 0, y: 4, w: PAGE_WIDTH + 80, h: 2, color: ACCENT_TINT },
        { type: "rect", x: 0, y: PAGE_HEIGHT - 6, w: PAGE_WIDTH + 80, h: 2, color: ACCENT_TINT },
        { type: "rect", x: 0, y: PAGE_HEIGHT - 4, w: PAGE_WIDTH + 80, h: 4, color: ACCENT },
      ],
    }),
    defaultStyle: { fontSize: 8, lineHeight: 1.15, color: TEXT },
    styles: {
      companyName: { fontSize: 12, bold: true, color: ACCENT, margin: [0, 0, 0, 2] },
      title: { fontSize: 24, alignment: "right" },
      th: { bold: true, color: "#000", fontSize: 7.5 },
      sectionLabel: { fontSize: 7, bold: true, color: ACCENT, characterSpacing: 0.6 },
      // Titre : gras, noir, taille 8 (bandeau gris fin) ; texte : noir, gras, taille 7,5 (lignes : 7).
      titleLine: { bold: true, color: "#000", fontSize: 8 },
      textLine: { bold: true, color: "#000", fontSize: 7.5 },
      optionTag: { color: ACCENT, fontSize: 7 },
      mono: { fontSize: 7 },
      conditions: { fontSize: 6.8, color: "#4b5058", lineHeight: 1.2 },
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
    return { bytes, warning: `Conditions générales de vente non ajoutées (${path}) : ${e instanceof Error ? e.message : String(e)}` };
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
