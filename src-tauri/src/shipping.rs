//! Tableau des frais de port par département (Réglages → Frais automatiques), lu dans un classeur :
//! colonnes Dpt | Destination | Prix transport, sur la première feuille (« CARTE TRANSPORT FRANCE » :
//! tarif CLASSIC d'une palette en colonne C). Les lignes sans prix (en-tête, titres, lignes vides)
//! sont ignorées.

use calamine::{open_workbook_auto, Data, Reader};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ShippingRate {
    /// Tel qu'écrit dans le tableau (« 4 », « 4 VILLES HAUTES MONTAGNES », « 2A »…).
    pub department: String,
    pub destination: String,
    pub price: f64,
}

pub fn read_rates(path: &Path) -> Result<Vec<ShippingRate>, String> {
    let mut wb = open_workbook_auto(path).map_err(|e| format!("Ouverture du fichier : {e}"))?;
    let sheet = wb.sheet_names().first().cloned().ok_or("Le classeur n'a aucune feuille.")?;
    let range = wb.worksheet_range(&sheet).map_err(|e| format!("Feuille « {sheet} » : {e}"))?;
    let rates = rates_from_rows(range.rows());
    if rates.is_empty() {
        return Err("Aucun tarif trouvé : il faut les colonnes Dpt, Destination et Prix sur la première feuille.".into());
    }
    Ok(rates)
}

/// Une seule ligne par numéro de département, la moins chère : « 4 » et « 4 VILLES HAUTES
/// MONTAGNES », ou deux lignes « 64 », ne donnent que le plus petit tarif.
fn rates_from_rows<'a>(rows: impl Iterator<Item = &'a [Data]>) -> Vec<ShippingRate> {
    let mut rates: Vec<ShippingRate> = Vec::new();
    for rate in rows.filter_map(parse_row) {
        let key = department_key(&rate.department);
        match rates.iter_mut().find(|r| department_key(&r.department) == key) {
            Some(kept) if rate.price < kept.price => *kept = rate,
            Some(_) => {}
            None => rates.push(rate),
        }
    }
    rates
}

/// Numéro du département (« 04 » → « 4 », « 2a » → « 2A », « 4 VILLES… » → « 4 ») ; sinon le libellé.
fn department_key(department: &str) -> String {
    let upper = department.trim().to_uppercase();
    let digits: String = upper.trim_start_matches('0').chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return upper;
    }
    match upper.trim_start_matches('0')[digits.len()..].chars().next() {
        Some(c @ ('A' | 'B')) => format!("{digits}{c}"),
        _ => digits,
    }
}

fn parse_row(row: &[Data]) -> Option<ShippingRate> {
    let department = text(row.first()?)?;
    // Prix à l'euro, comme affiché dans le classeur (qui garde les décimales du calcul : 53,182).
    let price = number(row.get(2)?)?.round();
    let destination = row.get(1).and_then(text).unwrap_or_default();
    Some(ShippingRate { department, destination, price })
}

fn text(cell: &Data) -> Option<String> {
    let s = match cell {
        Data::String(s) => s.split_whitespace().collect::<Vec<_>>().join(" "),
        Data::Float(f) if f.fract() == 0.0 => format!("{f:.0}"),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

fn number(cell: &Data) -> Option<f64> {
    match cell {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        Data::String(s) => s.trim().trim_end_matches('€').trim().replace(',', ".").parse().ok(),
        _ => None,
    }
    .filter(|p: &f64| *p > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_blank_rows_are_skipped() {
        let s = |v: &str| Data::String(v.into());
        let rows: Vec<Vec<Data>> = vec![
            vec![s("Dpt"), s("Destination"), s("Prix Transport")],
            vec![Data::Float(1.0), s("Ain"), Data::Float(53.182)],
            vec![s("64"), s("Pyrenees-Atlantiques"), Data::Float(70.277)],
            vec![s("64"), s("Pyrenees-Atlantiques"), Data::Float(54.183)],
            vec![s("4"), s("Alpes-de-Haute-Provence"), Data::Float(64.427)],
            vec![s("4  VILLES HAUTES MONTAGNES"), s("Alpes-de-Haute-Provence"), s("75,5")],
            vec![s("5 VILLES HAUTES MONTAGNES"), s("Hautes-Alpes"), s("74,8")],
            vec![s("05"), s("Hautes-Alpes"), s("64,4")],
            vec![s("2A"), s("Corse-du-Sud"), Data::Int(90)],
            vec![Data::Empty, Data::Empty, Data::Empty],
        ];
        let rates = rates_from_rows(rows.iter().map(|r| r.as_slice()));
        assert_eq!(
            rates,
            vec![
                ShippingRate { department: "1".into(), destination: "Ain".into(), price: 53.0 },
                ShippingRate { department: "64".into(), destination: "Pyrenees-Atlantiques".into(), price: 54.0 },
                ShippingRate { department: "4".into(), destination: "Alpes-de-Haute-Provence".into(), price: 64.0 },
                ShippingRate { department: "05".into(), destination: "Hautes-Alpes".into(), price: 64.0 },
                ShippingRate { department: "2A".into(), destination: "Corse-du-Sud".into(), price: 90.0 },
            ]
        );
    }
}
