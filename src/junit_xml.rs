//! Parsing des rapports JUnit/surefire (*.xml).
//!
//! Port du XmlParser.java d'origine : mêmes règles d'extraction
//! (suite = attribut "name" de la racine, statut déterminé par la présence
//! d'un enfant <skipped>/<failure>/<error>, temps en secondes).

use roxmltree::Document;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

impl TestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestStatus::Passed => "passed",
            TestStatus::Failed => "failed",
            TestStatus::Skipped => "skipped",
        }
    }
}

impl fmt::Display for TestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct TestCase {
    /// Libellé de suite à afficher. Pour un `<testsuite name="...">` au
    /// format Java classique (FQCN à points, ex: "org.outil.SomeTest"),
    /// c'est le dernier segment ("SomeTest"). Pour un `<testsuite>` dont le
    /// `name` est directement un chemin de fichier (Jest, Mocha, pytest...),
    /// c'est le nom de fichier complet, extension incluse (ex: "montest.test.js").
    pub suite: String,
    pub name: String,
    pub status: TestStatus,
    /// Temps en secondes, tel que rapporté par surefire.
    pub time: f32,
}

/// Extrait le dernier segment d'un chemin de fichier, quel que soit le
/// séparateur ('/' ou '\'), sans dépendre des conventions de chemin de
/// l'OS hôte (le rapport peut avoir été généré sur une autre plateforme
/// que celle qui l'analyse).
fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Xml { file: String, source: roxmltree::Error },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io(e) => write!(f, "erreur d'E/S: {e}"),
            ParseError::Xml { file, source } => write!(f, "XML invalide dans {file}: {source}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Dérive le libellé de suite à afficher à partir de l'attribut `name` du
/// `<testsuite>`.
///
/// Deux conventions coexistent dans l'écosystème JUnit XML :
/// - Style Java classique : `name` est un nom pleinement qualifié à points
///   (ex: "org.outil.SomeTest") -> on garde le dernier segment.
/// - Style "fichier de test" (Jest, Mocha, pytest, etc.) : `name` est
///   directement un chemin de fichier (ex:
///   "tests\integration\custom-queries\montest.test.js") -> le libellé devient
///   le nom de fichier complet, sans extension incluse ("montest.test").
fn derive_suite(full_name: &str) -> String {
    if full_name.contains('/') || full_name.contains('\\') {
        Path::new(basename(full_name))
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(full_name).to_string()
    } else {
        full_name.rsplit('.').next().unwrap_or(full_name).to_string()
    }
}

/// Parcourt un répertoire et parse tous les fichiers *.xml qu'il contient
/// (non récursif, comme `Files.newDirectoryStream(path, "*.xml")` côté Java).
pub fn parse_reports_dir(dir: &Path) -> Result<Vec<TestCase>, ParseError> {
    let mut cases = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(ParseError::Io)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xml"))
        .collect();
    entries.sort();

    for path in entries {
        let text = fs::read_to_string(&path).map_err(ParseError::Io)?;
        let doc = Document::parse(&text).map_err(|source| ParseError::Xml {
            file: path.display().to_string(),
            source,
        })?;

        // Deux formes de document coexistent dans l'écosystème JUnit XML :
        //   <testsuite name="..." ...> ... </testsuite>                (une seule suite)
        //   <testsuites name="..." ...> <testsuite .../> ... </testsuites>  (plusieurs)
        // `descendants()` inclut le nœud de départ lui-même, donc ce filtre
        // trouve chaque <testsuite> dans les deux cas — jamais le
        // <testsuites> englobant, qui n'a pas ce tag.
        for suite_node in doc.descendants().filter(|n| n.has_tag_name("testsuite")) {
            let full_name = suite_node.attribute("name").unwrap_or_default();
            let suite = derive_suite(full_name);

            // Enfants directs seulement : un <testcase> imbriqué dans une
            // sous-suite (rare, non-standard) sera traité par l'itération de
            // CETTE sous-suite, pas comptabilisé ici en double.
            for node in suite_node.children().filter(|n| n.has_tag_name("testcase")) {
                let name = node.attribute("name").unwrap_or_default().to_string();

                let time_attr = node.attribute("time").unwrap_or("0").replace(',', "");
                let time: f32 = time_attr.parse().unwrap_or(0.0);

                let mut status = TestStatus::Passed;
                for child in node.children().filter(|c| c.is_element()) {
                    match child.tag_name().name() {
                        "skipped" => status = TestStatus::Skipped,
                        "failure" | "error" => status = TestStatus::Failed,
                        _ => {}
                    }
                }

                cases.push(TestCase {
                    suite: suite.clone(),
                    name,
                    status,
                    time,
                });
            }
        }
    }

    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_and_parse(dir: &std::path::Path, filename: &str, xml: &str) -> Vec<TestCase> {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(filename), xml).unwrap();
        parse_reports_dir(dir).unwrap()
    }

    #[test]
    fn java_style_dotted_name_unaffected() {
        // Style Java classique (FQCN à points) : comportement inchangé.
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_xml_test_{}_a", std::process::id()));
        let xml = r#"<?xml version="1.0"?>
<testsuite name="org.outil.SampleTest" tests="1">
  <testcase name="works" time="0.1"/>
</testsuite>"#;
        let cases = write_and_parse(&tmp, "a.xml", xml);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].suite, "SampleTest");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_path_style_name_windows_separators() {
        // Exemple exact rapporté : name = chemin de fichier avec '\', pas de
        // FQCN à points. Cas des rapports générés par Jest/Mocha/etc.
        // Le libellé de suite est le nom de fichier complet, tel quel.
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_xml_test_{}_b", std::process::id()));
        let xml = r#"<?xml version="1.0"?>
<testsuite name="tests\integration\custom-queries\montest.test.js" errors="0" failures="0" skipped="0" timestamp="2026-08-13T11:52:52" time="5.768" tests="1">
  <testcase name="does the thing" time="1.2"/>
</testsuite>"#;
        let cases = write_and_parse(&tmp, "b.xml", xml);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].suite, "montest.test.js");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_path_style_name_unix_separators() {
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_xml_test_{}_c", std::process::id()));
        let xml = r#"<?xml version="1.0"?>
<testsuite name="tests/integration/custom-queries/montest.test.js" tests="1">
  <testcase name="does the thing" time="1.2"/>
</testsuite>"#;
        let cases = write_and_parse(&tmp, "c.xml", xml);
        assert_eq!(cases[0].suite, "montest.test.js");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_path_style_all_cases_in_suite_share_same_label() {
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_xml_test_{}_d", std::process::id()));
        let xml = r#"<?xml version="1.0"?>
<testsuite name="tests/unit/montest.test.js" tests="2">
  <testcase name="one" time="0.1"/>
  <testcase name="two" time="0.2"/>
</testsuite>"#;
        let cases = write_and_parse(&tmp, "d.xml", xml);
        assert_eq!(cases.len(), 2);
        assert!(cases.iter().all(|c| c.suite == "montest.test.js"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn wrapped_testsuites_englobant_is_never_used_as_suite() {
        // Structure exacte rapportée : <testsuites> englobant, <testsuite>
        // avec name = chemin de fichier.
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_xml_test_{}_e", std::process::id()));
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="Integration tests" tests="78" failures="0" errors="0" time="39.358">
  <testsuite name="tests\integration\custom-queries\montest.test.js" errors="0" failures="0" skipped="0" timestamp="2026-08-13T11:52:52" time="5.768" tests="2">
    <testcase classname="tests\integration\custom-queries\montest.test.js &gt; Procédure proc_WhereAmIInstalled" name="Test pour neo4jback" time="0.394" file="tests\integration\custom-queries\montest.test.js">
    </testcase>
    <testcase classname="tests\integration\custom-queries\montest.test.js &gt; Procédure proc_WhereAmIInstalled" name="Test pour connexionmiddle" time="0.307" file="tests\integration\custom-queries\montest.test.js">
      <failure message="boom">stack</failure>
    </testcase>
  </testsuite>
  <testsuite name="tests\integration\other\other.test.js" errors="0" failures="0" skipped="0" time="1.0" tests="1">
    <testcase classname="tests\integration\other\other.test.js &gt; Truc" name="Un autre test" time="0.5" file="tests\integration\other\other.test.js">
    </testcase>
  </testsuite>
</testsuites>"#;
        let cases = write_and_parse(&tmp, "e.xml", xml);

        // 3 testcases au total, PAS "Integration tests" comme suite pour tous
        assert_eq!(cases.len(), 3);
        assert!(
            cases.iter().all(|c| c.suite != "Integration tests"),
            "le nom du <testsuites> englobant ne doit jamais être utilisé comme suite"
        );

        let montest_cases: Vec<_> = cases.iter().filter(|c| c.name.contains("neo4jback") || c.name.contains("connexionmiddle")).collect();
        assert_eq!(montest_cases.len(), 2);
        assert!(montest_cases.iter().all(|c| c.suite == "montest.test.js"));

        let other_case = cases.iter().find(|c| c.name == "Un autre test").unwrap();
        assert_eq!(other_case.suite, "other.test.js");

        // Statuts bien lus malgré la structure d'itération par sous-suite
        let statuses: Vec<_> = cases.iter().map(|c| c.status).collect();
        assert_eq!(statuses.iter().filter(|s| **s == TestStatus::Passed).count(), 2);
        assert_eq!(statuses.iter().filter(|s| **s == TestStatus::Failed).count(), 1);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn single_testsuite_root_without_wrapper_still_works() {
        // S'assure que le format à une seule suite (root = <testsuite>,
        // pas de <testsuites> englobant) continue de fonctionner.
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_xml_test_{}_f", std::process::id()));
        let xml = r#"<?xml version="1.0"?>
<testsuite name="org.outil.SampleTest" tests="1">
  <testcase name="works" time="0.1"/>
</testsuite>"#;
        let cases = write_and_parse(&tmp, "f.xml", xml);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].suite, "SampleTest");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn basename_handles_both_separators_and_plain_name() {
        assert_eq!(basename("a/b/c.java"), "c.java");
        assert_eq!(basename("a\\b\\c.java"), "c.java");
        assert_eq!(basename("c.java"), "c.java");
        assert_eq!(basename(""), "");
    }

    #[test]
    fn derive_suite_edge_cases() {
        assert_eq!(derive_suite("tests/integration/noext"), "noext");
        assert_eq!(derive_suite("tests/.hidden"), ".hidden");
        assert_eq!(derive_suite("org.outil.SomeTest"), "SomeTest");
        assert_eq!(derive_suite("SomeTest"), "SomeTest");
    }
}
