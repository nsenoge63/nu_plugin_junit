//! Génération du rapport Excel (.xlsx) à partir d'une liste de `TestCase`.
//!
//! Port de ExcelGenerator.java : mêmes colonnes, mêmes couleurs par statut,
//! fusion des cellules de nom de suite, formules COUNTIF pour les totaux.
//! Différence : format .xlsx (OOXML) au lieu de .xls (HSSF/binaire) — les
//! couleurs indexées HSSF d'origine sont approximées en RGB, pas identiques
//! au pixel près.

use crate::junit_xml::{TestCase, TestStatus};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, XlsxError};

pub struct ReportMeta {
    pub project_name: String,
    pub branch: String,
    pub generator_version: String,
}

/// Écrit le classeur sur `path`. Les `cases` doivent être fournis dans
/// l'ordre où les suites doivent apparaître (elles sont regroupées par
/// suite consécutive, comme le faisait le code Java d'origine).
pub fn write_report(
    path: &str,
    cases: &[TestCase],
    meta: &ReportMeta,
) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("JUnit Report")?;

    // --- Styles ------------------------------------------------------
    let header_fmt = Format::new()
        .set_background_color(Color::RGB(0xCCE5FF))
        .set_border(FormatBorder::Thin)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_bold();

    let white_fmt = Format::new()
        .set_background_color(Color::White)
        .set_border(FormatBorder::Thin)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);

    let grey_fmt = Format::new()
        .set_background_color(Color::RGB(0xE0E0E0))
        .set_border(FormatBorder::Thin)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);

    let status_fmt = |status: TestStatus| {
        let color = match status {
            TestStatus::Passed => Color::RGB(0x00CC66),
            TestStatus::Failed => Color::RGB(0xFF3333),
            TestStatus::Skipped => Color::RGB(0xFFA500),
        };
        Format::new()
            .set_background_color(color)
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
    };

    let plain_fmt = Format::new().set_align(FormatAlign::Left);
    let plain_fmt_right = Format::new().set_align(FormatAlign::Right);

    // --- En-tête -------------------------------------------------------
    sheet.write_string_with_format(0, 0, "Classe de test", &header_fmt)?;
    sheet.write_string_with_format(0, 1, "Nom du test", &header_fmt)?;
    sheet.write_string_with_format(0, 2, "Résultat du test", &header_fmt)?;
    sheet.write_string_with_format(0, 3, "Temps (en minutes)", &header_fmt)?;

    // --- Regroupement par suite consécutive (comme TestSuite en Java) --
    let mut groups: Vec<(String, Vec<&TestCase>)> = Vec::new();
    for case in cases {
        match groups.last_mut() {
            Some((suite, items)) if suite == &case.suite => items.push(case),
            _ => groups.push((case.suite.clone(), vec![case])),
        }
    }

    let mut row: u32 = 1; // ligne 0 = en-tête
    let mut total_time: f64 = 0.0;
    let mut suite_index = 0usize;

    for (suite_name, items) in &groups {
        let start_row = row;
        let band_fmt = if suite_index % 2 == 0 { &white_fmt } else { &grey_fmt };

        for case in items {
            sheet.write_string_with_format(row, 0, suite_name, band_fmt)?;
            sheet.write_string_with_format(row, 1, &case.name, band_fmt)?;
            sheet.write_string_with_format(row, 2, case.status.as_str(), &status_fmt(case.status))?;

            let time = case.time as f64;
            if time <= 1.0 {
                sheet.write_string_with_format(row, 3, "< 1 s", band_fmt)?;
            } else {
                sheet.write_string_with_format(row, 3, &format_mmss(time), band_fmt)?;
            }

            total_time += time;
            row += 1;
        }

        // Fusionne la colonne "Classe de test" si la suite a plus d'une ligne
        if row - 1 > start_row {
            sheet.merge_range(start_row, 0, row - 1, 0, suite_name, band_fmt)?;
        }

        suite_index += 1;
    }

    // `row` (0-indexé) pointe juste après la dernière ligne de données écrite ;
    // en notation Excel (1-indexée), la dernière ligne de données est donc `row`
    // et la première est la ligne 2 (juste après l'en-tête, ligne 1).
    let last_data_row_1indexed = row;
    let first_data_row_1indexed = 2u32;

    // --- Pied de page : infos projet, totaux, durée, version -----------
    row += 2;
    sheet.write_string_with_format(row, 2, "Nom du projet", &plain_fmt)?;
    sheet.write_string_with_format(row, 3, &meta.project_name, &plain_fmt_right)?;
    row += 1;
    sheet.write_string_with_format(row, 2, "Branche", &plain_fmt)?;
    sheet.write_string_with_format(row, 3, &meta.branch, &plain_fmt_right)?;
    row += 2;

    for status in [TestStatus::Passed, TestStatus::Failed, TestStatus::Skipped] {
        sheet.write_string_with_format(row, 2, &capitalize(status.as_str()), &plain_fmt)?;
        let formula = format!(
            "=COUNTIF(C{}:C{},\"{}\")",
            first_data_row_1indexed,
            last_data_row_1indexed,
            status.as_str()
        );
        sheet.write_formula_with_format(row, 3, formula.as_str(), &plain_fmt_right)?;
        row += 1;
    }

    row += 1;
    sheet.write_string_with_format(row, 2, "Durée totale des tests", &plain_fmt)?;
    sheet.write_string_with_format(row, 3, &format_hhmmss(total_time), &plain_fmt_right)?;
    row += 2;
    sheet.write_string_with_format(
        row,
        0,
        &format!("Version du générateur : {}", meta.generator_version),
        &plain_fmt,
    )?;

    sheet.autofit();
    workbook.save(path)?;

    Ok(())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn format_mmss(seconds: f64) -> String {
    let total = seconds.round() as i64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn format_hhmmss(seconds: f64) -> String {
    let total = seconds.round() as i64;
    format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60)
}

