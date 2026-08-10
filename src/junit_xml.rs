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
    /// Nom court de la suite (dernier segment du nom pleinement qualifié,
    /// ex: "cnaf.ocda.SomeTest" -> "SomeTest"), comme TestSuite.getTestName().
    pub suite: String,
    pub name: String,
    pub status: TestStatus,
    /// Temps en secondes, tel que rapporté par surefire.
    pub time: f32,
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

        let root = doc.root_element();
        let full_name = root.attribute("name").unwrap_or_default();
        let suite = full_name.rsplit('.').next().unwrap_or(full_name).to_string();

        for node in doc.descendants().filter(|n| n.has_tag_name("testcase")) {
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

    Ok(cases)
}
