//! Export d'une liste de prix vers Excel à partir d'un gabarit `.xlsx` choisi dans les réglages.
//!
//! Le gabarit ne sert que pour le haut de la feuille (logo, titre, bloc client) : tout ce qui
//! précède la ligne d'en-tête « Désignation » (colonne A) est gardé tel quel, avec ses styles.
//! Les marqueurs `_NOM_CLIENT_`, `_EMAIL_`, `_NOM_`, `_PRENOM_`, `_CODE_` sont remplacés.
//! L'en-tête et les lignes sont ensuite réécrits avec les données de l'appli
//! (Désignation | Nom. Enedis | Référence | Prix LPN HT), dans les styles du gabarit.
//!
//! Le fichier reste cohérent pour Excel : fusions, filtre automatique et zone de filtre suivent
//! les nouvelles lignes, la chaîne de calcul (calcChain) des anciennes formules est retirée.

use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Bloc client du haut de la feuille.
#[derive(Default)]
pub struct Header {
    pub client_name: String,
    pub client_code: String,
    pub email: String,
    pub last_name: String,
    pub first_name: String,
}

/// Une ligne de la liste de prix.
pub struct Row {
    pub designation: String,
    pub enedis_code: String,
    pub product_ref: String,
    pub price: f64,
}

const COLUMNS: [&str; 4] = ["Désignation", "Nom. Enedis", "Référence", "Prix LPN HT"];

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Remplit le gabarit et renvoie le classeur obtenu.
pub fn fill_template(template: &[u8], header: &Header, rows: &[Row]) -> Result<Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(template)).map_err(|e| format!("Gabarit illisible : {e}"))?;
    let read = |zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str| -> Result<Option<String>> {
        match zip.by_name(name) {
            Ok(mut f) => {
                let mut s = String::new();
                f.read_to_string(&mut s).map_err(err)?;
                Ok(Some(s))
            }
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(e) => Err(err(e)),
        }
    };

    let workbook = read(&mut zip, "xl/workbook.xml")?.ok_or("Gabarit : xl/workbook.xml manquant")?;
    let rels = read(&mut zip, "xl/_rels/workbook.xml.rels")?.ok_or("Gabarit : relations manquantes")?;
    let sheet_path = first_sheet_path(&workbook, &rels)?;
    let sheet = read(&mut zip, &sheet_path)?.ok_or("Gabarit : feuille introuvable")?;
    let shared = read(&mut zip, "xl/sharedStrings.xml")?;
    let strings = shared.as_deref().map(shared_strings).unwrap_or_default();
    let styles = read(&mut zip, "xl/styles.xml")?;

    // Prix : style de la colonne prix du gabarit, avec 2 décimales (« 100,00 »).
    let mut new_styles = None;
    let (sheet, range) = rewrite_sheet(&sheet, &strings, rows, |base| {
        let (xml, index) = add_price_style(styles.as_deref()?, base.as_deref())?;
        new_styles = Some(xml);
        Some(index)
    })?;

    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(err)?;
        let name = file.name().to_string();
        if name == "xl/calcChain.xml" || file.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(err)?;
        let text = || String::from_utf8_lossy(&bytes).into_owned();
        let replaced = match name.as_str() {
            n if n == sheet_path => Some(sheet.clone()),
            "xl/sharedStrings.xml" => Some(fill_placeholders(&text(), header)),
            "xl/styles.xml" => new_styles.clone(),
            "xl/workbook.xml" => Some(update_filter_name(&text(), &range)),
            "xl/_rels/workbook.xml.rels" => Some(remove_calc_chain_rel(&text())),
            "[Content_Types].xml" => Some(text().replace(
                r#"<Override PartName="/xl/calcChain.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml"/>"#,
                "",
            )),
            _ => None,
        };
        out.start_file(&name, options).map_err(err)?;
        out.write_all(replaced.as_ref().map(|s| s.as_bytes()).unwrap_or(&bytes)).map_err(err)?;
    }
    Ok(out.finish().map_err(err)?.into_inner())
}

/// Chemin de la première feuille du classeur (d'après workbook.xml et ses relations).
fn first_sheet_path(workbook: &str, rels: &str) -> Result<String> {
    let sheet = between(workbook, "<sheet ", "/>").ok_or("Gabarit : aucune feuille")?;
    let id = attr(sheet, "r:id").ok_or("Gabarit : feuille sans identifiant")?;
    let rel = rels
        .split("<Relationship ")
        .find(|r| attr(r, "Id").as_deref() == Some(id.as_str()))
        .ok_or("Gabarit : relation de la feuille introuvable")?;
    let target = attr(rel, "Target").ok_or("Gabarit : cible de la feuille introuvable")?;
    Ok(match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => format!("xl/{target}"),
    })
}

/// Textes partagés, dans l'ordre (texte riche : morceaux recollés).
fn shared_strings(xml: &str) -> Vec<String> {
    xml.split("<si>")
        .skip(1)
        .map(|si| {
            let si = si.split("</si>").next().unwrap_or("");
            si.split("<t")
                .skip(1)
                .filter_map(|t| t.split_once('>').map(|(_, rest)| rest.split("</t>").next().unwrap_or("")))
                .map(unescape)
                .collect()
        })
        .collect()
}

/// Nouvelle feuille, et zone des données (`A12:D140`) pour le filtre.
/// `price_style` : style des prix, d'après celui de la colonne prix du gabarit.
fn rewrite_sheet(
    sheet: &str,
    strings: &[String],
    rows: &[Row],
    price_style: impl FnOnce(Option<String>) -> Option<String>,
) -> Result<(String, String)> {
    let start = sheet.find("<sheetData").ok_or("Gabarit : feuille sans données")?;
    let open_end = start + sheet[start..].find('>').ok_or("Gabarit : feuille illisible")? + 1;
    let (before, rest) = (&sheet[..start], &sheet[open_end..]);
    let close = rest.find("</sheetData>").unwrap_or(0);
    let (data, after) = if sheet[start..open_end].ends_with("/>") {
        ("", rest)
    } else {
        (&rest[..close], &rest[close + "</sheetData>".len()..])
    };

    let template_rows: Vec<(u32, &str)> = data
        .split("<row ")
        .skip(1)
        .filter_map(|r| {
            let r = r.split("</row>").next().unwrap_or(r);
            Some((attr(r, "r")?.parse().ok()?, r))
        })
        .collect();

    // Ligne d'en-tête : « Désignation » en colonne A ; à défaut, deux lignes sous la dernière.
    let header_row = template_rows
        .iter()
        .find(|(n, r)| cell_text(r, &format!("A{n}"), strings).is_some_and(|t| t.trim() == "Désignation"))
        .map(|(n, _)| *n);
    let last_template_row = template_rows.last().map(|(n, _)| *n).unwrap_or(0);
    let h = header_row.unwrap_or(last_template_row + 2);

    // Styles : ceux de l'en-tête du gabarit, et de la première ligne produit (référence en C).
    let style_of = |n: u32, col: &str| {
        template_rows
            .iter()
            .find(|(r, _)| *r == n)
            .and_then(|(_, r)| cell(r, &format!("{col}{n}")))
            .and_then(|c| attr(c, "s"))
    };
    let first_item = template_rows
        .iter()
        .find(|(n, r)| *n > h && cell_text(r, &format!("C{n}"), strings).is_some_and(|t| !t.trim().is_empty()))
        .map(|(n, _)| *n);
    let head_styles: Vec<Option<String>> = ["A", "B", "C", "D"].iter().map(|c| style_of(h, c)).collect();
    let mut item_styles: Vec<Option<String>> = ["A", "B", "C", "D"]
        .iter()
        .map(|c| first_item.and_then(|n| style_of(n, c)))
        .collect();
    item_styles[3] = price_style(item_styles[3].take());

    let mut xml = String::with_capacity(data.len());
    for (n, r) in &template_rows {
        if *n < h {
            xml.push_str("<row ");
            xml.push_str(r);
            xml.push_str("</row>");
        }
    }
    let style = |s: &Option<String>| s.as_ref().map(|s| format!(r#" s="{s}""#)).unwrap_or_default();
    let text_cell = |r: &str, s: &Option<String>, v: &str| {
        format!(r#"<c r="{r}"{} t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, style(s), escape(v))
    };
    xml.push_str(&format!(r#"<row r="{h}">"#));
    for (i, (col, title)) in ["A", "B", "C", "D"].iter().zip(COLUMNS).enumerate() {
        xml.push_str(&text_cell(&format!("{col}{h}"), &head_styles[i], title));
    }
    xml.push_str("</row>");
    for (i, row) in rows.iter().enumerate() {
        let n = h + 1 + i as u32;
        xml.push_str(&format!(r#"<row r="{n}">"#));
        xml.push_str(&text_cell(&format!("A{n}"), &item_styles[0], &row.designation));
        xml.push_str(&text_cell(&format!("B{n}"), &item_styles[1], &row.enedis_code));
        xml.push_str(&text_cell(&format!("C{n}"), &item_styles[2], &row.product_ref));
        xml.push_str(&format!(r#"<c r="D{n}"{}><v>{}</v></c>"#, style(&item_styles[3]), row.price));
        xml.push_str("</row>");
    }
    let last = h + rows.len() as u32;
    let range = format!("A{h}:D{last}");

    // Après les données : fusions et filtre limités au haut de la feuille / aux nouvelles lignes.
    let mut after = keep_merges_above(after, h);
    after = replace_attr_value(&after, "<autoFilter ", "ref", &range);
    let before = replace_attr_value(before, "<dimension ", "ref", &format!("A1:F{last}"));
    Ok((format!("{before}<sheetData>{xml}</sheetData>{after}"), range))
}

/// Ajoute aux styles une copie du style `base` (ou d'un style neutre) au format nombre à
/// 2 décimales (`#,##0.00`, format intégré n° 4) ; renvoie les styles et l'index du nouveau.
fn add_price_style(styles: &str, base: Option<&str>) -> Option<(String, String)> {
    let start = styles.find("<cellXfs")?;
    let open_end = start + styles[start..].find('>')? + 1;
    let end = styles[open_end..].find("</cellXfs>")? + open_end;
    // Éléments <xf …/> ou <xf …>…</xf> (y compris <xf/> sans attribut).
    let xfs: Vec<&str> = styles[open_end..end]
        .split("<xf")
        .skip(1)
        .filter(|x| x.starts_with([' ', '/', '>']))
        .collect();
    let template = base
        .and_then(|b| b.parse::<usize>().ok())
        .and_then(|i| xfs.get(i))
        .map(|xf| format!("<xf{}", xf.trim_end()))
        .filter(|xf| xf.contains("numFmtId="))
        .unwrap_or_else(|| r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#.to_string());
    let with_format = replace_attr_value(&template, "<xf ", "numFmtId", "4");
    let xf = if with_format.contains("applyNumberFormat=") {
        replace_attr_value(&with_format, "<xf ", "applyNumberFormat", "1")
    } else {
        with_format.replacen("<xf ", r#"<xf applyNumberFormat="1" "#, 1)
    };
    let index = xfs.len();
    let open = replace_attr_value(&styles[start..open_end], "<cellXfs", "count", &(index + 1).to_string());
    Some((format!("{}{open}{}{xf}{}", &styles[..start], &styles[open_end..end], &styles[end..]), index.to_string()))
}

/// Remplace les marqueurs du haut de la feuille (textes partagés).
fn fill_placeholders(xml: &str, h: &Header) -> String {
    // _NOM_CLIENT_ avant _NOM_, qui en est le début.
    [
        ("_NOM_CLIENT_", &h.client_name),
        ("_PRENOM_", &h.first_name),
        ("_NOM_", &h.last_name),
        ("_EMAIL_", &h.email),
        ("_CODE_", &h.client_code),
    ]
    .iter()
    .fold(xml.to_string(), |s, (k, v)| s.replace(k, &escape(v)))
}

/// Zone du filtre automatique (`_xlnm._FilterDatabase`) : les nouvelles lignes.
fn update_filter_name(workbook: &str, range: &str) -> String {
    let Some(start) = workbook.find(r#"name="_xlnm._FilterDatabase""#) else { return workbook.to_string() };
    let Some(gt) = workbook[start..].find('>').map(|i| start + i + 1) else { return workbook.to_string() };
    let Some(end) = workbook[gt..].find("</definedName>").map(|i| gt + i) else { return workbook.to_string() };
    let current = &workbook[gt..end];
    let sheet = current.rsplit_once('!').map(|(s, _)| s).unwrap_or("");
    let (a, b) = range.split_once(':').unwrap_or((range, range));
    let abs = |c: &str| {
        let (col, row) = c.split_at(c.find(|ch: char| ch.is_ascii_digit()).unwrap_or(c.len()));
        format!("${col}${row}")
    };
    format!("{}{sheet}!{}:{}{}", &workbook[..gt], abs(a), abs(b), &workbook[end..])
}

fn remove_calc_chain_rel(rels: &str) -> String {
    rels.split_inclusive("/>")
        .filter(|r| !r.contains("relationships/calcChain"))
        .collect()
}

/// Garde les fusions entièrement au-dessus de la ligne `h` (le haut du gabarit).
fn keep_merges_above(xml: &str, h: u32) -> String {
    let Some(start) = xml.find("<mergeCells") else { return xml.to_string() };
    let Some(end) = xml.find("</mergeCells>").map(|i| i + "</mergeCells>".len()) else { return xml.to_string() };
    let kept: Vec<&str> = xml[start..end]
        .split("<mergeCell ")
        .skip(1)
        .filter(|m| {
            attr(m, "ref").is_some_and(|r| {
                r.split(':').all(|c| c.trim_start_matches(|ch: char| ch.is_ascii_alphabetic()).parse::<u32>().is_ok_and(|n| n < h))
            })
        })
        .collect();
    let block = if kept.is_empty() {
        String::new()
    } else {
        let cells: String = kept
            .iter()
            .map(|m| format!("<mergeCell {}", m.split("/>").next().unwrap_or("").to_string() + "/>"))
            .collect();
        format!(r#"<mergeCells count="{}">{cells}</mergeCells>"#, kept.len())
    };
    format!("{}{block}{}", &xml[..start], &xml[end..])
}

// ---------- Petits outils XML (le gabarit est un fichier Excel standard) ----------

/// Valeur d'un attribut `name="…"` dans un fragment de balise.
fn attr(fragment: &str, name: &str) -> Option<String> {
    let tag = fragment.split('>').next().unwrap_or(fragment);
    let key = format!(" {name}=\"");
    let padded = format!(" {tag}");
    let i = padded.find(&key)? + key.len();
    let end = padded[i..].find('"')? + i;
    Some(padded[i..end].to_string())
}

fn replace_attr_value(xml: &str, tag: &str, name: &str, value: &str) -> String {
    let Some(start) = xml.find(tag) else { return xml.to_string() };
    let Some(close) = xml[start..].find('>').map(|i| start + i) else { return xml.to_string() };
    let key = format!(" {name}=\"");
    let Some(i) = xml[start..close].find(&key).map(|i| start + i + key.len()) else { return xml.to_string() };
    let Some(end) = xml[i..].find('"').map(|e| i + e) else { return xml.to_string() };
    format!("{}{value}{}", &xml[..i], &xml[end..])
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let i = s.find(open)? + open.len();
    let j = s[i..].find(close)? + i;
    Some(&s[i..j])
}

/// Fragment `<c r="A12" …>…</c>` d'une cellule dans le contenu d'une ligne.
fn cell<'a>(row: &'a str, r: &str) -> Option<&'a str> {
    let key = format!("<c r=\"{r}\"");
    let i = row.find(&key)?;
    let rest = &row[i..];
    let end = if rest.split('>').next()?.ends_with('/') {
        rest.find("/>")? + 2
    } else {
        rest.find("</c>")? + 4
    };
    Some(&rest[..end])
}

/// Texte d'une cellule (texte partagé, texte en ligne ou valeur).
fn cell_text(row: &str, r: &str, strings: &[String]) -> Option<String> {
    let c = cell(row, r)?;
    if attr(c, "t").as_deref() == Some("s") {
        let i: usize = between(c, "<v>", "</v>")?.parse().ok()?;
        return strings.get(i).cloned();
    }
    if let Some(t) = between(c, "<t>", "</t>").or_else(|| between(c, "<t xml:space=\"preserve\">", "</t>")) {
        return Some(unescape(t));
    }
    between(c, "<v>", "</v>").map(unescape)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gabarit minimal : titre, marqueurs, en-tête « Désignation » en ligne 4, 2 anciennes lignes,
    /// une fusion dans le haut et une dans les données, filtre, chaîne de calcul.
    fn template() -> Vec<u8> {
        let files: [(&str, &str); 7] = [
            ("xl/styles.xml", r#"<styleSheet><cellXfs count="16"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf/><xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment horizontal="center"/></xf></cellXfs></styleSheet>"#),
            ("[Content_Types].xml", r#"<Types><Override PartName="/xl/calcChain.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml"/></Types>"#),
            ("xl/workbook.xml", r#"<workbook><sheets><sheet name="Références" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="_xlnm._FilterDatabase" localSheetId="0" hidden="1">Références!$A$4:$F$6</definedName></definedNames></workbook>"#),
            ("xl/_rels/workbook.xml.rels", r#"<Relationships><Relationship Id="rId1" Type="x/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain" Target="calcChain.xml"/></Relationships>"#),
            ("xl/sharedStrings.xml", r#"<sst><si><t>Liste de prix</t></si><si><t>_NOM_CLIENT_</t></si><si><t>email : _EMAIL_</t></si><si><t>Désignation</t></si><si><t>Vieux produit</t></si></sst>"#),
            ("xl/worksheets/sheet1.xml", concat!(
                r#"<worksheet><dimension ref="A1:F6"/><sheetData>"#,
                r#"<row r="1"><c r="A1" s="9" t="s"><v>0</v></c></row>"#,
                r#"<row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2" t="s"><v>2</v></c></row>"#,
                r#"<row r="4"><c r="A4" s="3" t="s"><v>3</v></c><c r="B4" s="7"/><c r="C4" s="7"/><c r="D4" s="16"/></row>"#,
                r#"<row r="5"><c r="A5" s="4" t="s"><v>4</v></c></row>"#,
                r#"<row r="6"><c r="A6" s="5" t="s"><v>4</v></c><c r="B6" s="1"><v>1</v></c><c r="C6" s="1"><v>2</v></c><c r="D6" s="15"><v>3</v></c><c r="E6" s="15"><f>D6*2</f><v>6</v></c></row>"#,
                r#"</sheetData><autoFilter ref="A4:F6"/><mergeCells count="2"><mergeCell ref="A1:F1"/><mergeCell ref="A5:C5"/></mergeCells><drawing r:id="rId1"/></worksheet>"#,
            )),
            ("xl/calcChain.xml", r#"<calcChain><c r="E6" i="1"/></calcChain>"#),
        ];
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in files {
            w.start_file(name, SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn unzip(bytes: &[u8]) -> std::collections::HashMap<String, String> {
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..z.len())
            .map(|i| {
                let mut f = z.by_index(i).unwrap();
                let mut s = String::new();
                f.read_to_string(&mut s).unwrap();
                (f.name().to_string(), s)
            })
            .collect()
    }

    #[test]
    fn fills_header_and_replaces_rows() {
        let header = Header { client_name: "ELEC & SUD".into(), client_code: "C001".into(), email: "a@b.fr".into(), ..Default::default() };
        let rows = [
            Row { designation: "Coffret <borne>".into(), enedis_code: "69.80.540".into(), product_ref: "0451050".into(), price: 135.72 },
            Row { designation: "Grille".into(), enedis_code: String::new(), product_ref: "0540013R13".into(), price: 250.0 },
        ];
        let out = unzip(&fill_template(&template(), &header, &rows).unwrap());
        let sheet = &out["xl/worksheets/sheet1.xml"];

        // Haut gardé (titre, styles), marqueurs remplis (et échappés).
        assert!(sheet.contains(r#"<row r="1"><c r="A1" s="9" t="s"><v>0</v></c></row>"#));
        assert!(out["xl/sharedStrings.xml"].contains("<t>ELEC &amp; SUD</t>"));
        assert!(out["xl/sharedStrings.xml"].contains("email : a@b.fr"));
        // En-tête et lignes de l'appli, dans les styles du gabarit ; anciennes lignes et formules retirées.
        assert!(sheet.contains(r#"<c r="D4" s="16" t="inlineStr"><is><t xml:space="preserve">Prix LPN HT</t></is></c>"#));
        assert!(sheet.contains(r#"<c r="A5" s="5" t="inlineStr"><is><t xml:space="preserve">Coffret &lt;borne&gt;</t></is></c>"#));
        // Prix : copie du style 15 du gabarit, au format 2 décimales.
        assert!(sheet.contains(r#"<c r="D6" s="16"><v>250</v></c>"#));
        let styles = &out["xl/styles.xml"];
        assert!(styles.contains(r#"<cellXfs count="17">"#));
        assert!(styles.ends_with(r#"<xf applyNumberFormat="1" numFmtId="4" fontId="2" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment horizontal="center"/></xf></cellXfs></styleSheet>"#));
        assert!(!sheet.contains("<f>") && !sheet.contains(r#"r="E6""#));
        // Fusions, filtre, dimension et zone de filtre suivent les nouvelles lignes.
        assert!(sheet.contains(r#"<mergeCells count="1"><mergeCell ref="A1:F1"/></mergeCells>"#));
        assert!(sheet.contains(r#"<autoFilter ref="A4:D6"/>"#));
        assert!(sheet.contains(r#"<dimension ref="A1:F6"/>"#));
        assert!(out["xl/workbook.xml"].contains("Références!$A$4:$D$6</definedName>"));
        // Chaîne de calcul retirée partout.
        assert!(!out.contains_key("xl/calcChain.xml"));
        assert!(!out["xl/_rels/workbook.xml.rels"].contains("calcChain"));
        assert!(!out["[Content_Types].xml"].contains("calcChain"));
    }
}
