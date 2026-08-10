//! nu_plugin_junit — plugin nushell natif pour parser des rapports
//! JUnit/surefire et produire un rapport Excel stylé, directement dans
//! le pipeline nushell.
//!
//! Commandes exposées :
//!   junit report <dir>              -> table structurée (suite/test/status/time)
//!   junit to-xlsx <path> [--project ..] [--branch ..]  -> écrit un .xlsx stylé

mod junit_xml;
mod xlsx_report;

use junit_xml::{parse_reports_dir, TestCase, TestStatus};
use nu_plugin::{
    serve_plugin, EngineInterface, EvaluatedCall, MsgPackSerializer, Plugin, PluginCommand,
    SimplePluginCommand,
};
use nu_protocol::{Category, LabeledError, Record, Signature, Span, SyntaxShape, Type, Value};
use std::path::PathBuf;
use xlsx_report::{write_report, ReportMeta};

struct JunitPlugin;

impl Plugin for JunitPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![Box::new(JunitReport), Box::new(JunitToXlsx)]
    }
}

// ---------------------------------------------------------------------
// junit report <dir>
// ---------------------------------------------------------------------

struct JunitReport;

impl SimplePluginCommand for JunitReport {
    type Plugin = JunitPlugin;

    fn name(&self) -> &str {
        "junit report"
    }

    fn description(&self) -> &str {
        "Parse des rapports JUnit/surefire (*.xml) et retourne une table structurée"
    }

    fn signature(&self) -> Signature {
        Signature::build(SimplePluginCommand::name(self))
            .optional(
                "path",
                SyntaxShape::Directory,
                "Répertoire des rapports XML (défaut : target/surefire-reports)",
            )
            .input_output_type(Type::Nothing, Type::table())
            .category(Category::Formats)
    }

    fn run(
        &self,
        _plugin: &JunitPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let span = call.head;
        let dir: Option<String> = call.opt(0)?;
        let dir = dir.unwrap_or_else(|| "target/surefire-reports".to_string());
        let dir_path = PathBuf::from(&dir);

        if !dir_path.is_dir() {
            return Err(LabeledError::new(format!("Répertoire introuvable : {dir}"))
                .with_label("chemin invalide", span));
        }

        let cases = parse_reports_dir(&dir_path).map_err(|e| {
            LabeledError::new(format!("Erreur de lecture des rapports : {e}"))
                .with_label("erreur de parsing", span)
        })?;

        let records: Vec<Value> = cases
            .into_iter()
            .map(|c| case_to_value(&c, span))
            .collect();

        Ok(Value::list(records, span))
    }
}

fn case_to_value(c: &TestCase, span: Span) -> Value {
    let mut rec = Record::new();
    rec.push("suite", Value::string(c.suite.clone(), span));
    rec.push("test", Value::string(c.name.clone(), span));
    rec.push("status", Value::string(c.status.as_str(), span));
    rec.push("time", Value::float(c.time as f64, span));
    Value::record(rec, span)
}

// ---------------------------------------------------------------------
// junit to-xlsx <path> [--project ..] [--branch ..]
// ---------------------------------------------------------------------

struct JunitToXlsx;

impl SimplePluginCommand for JunitToXlsx {
    type Plugin = JunitPlugin;

    fn name(&self) -> &str {
        "junit to-xlsx"
    }

    fn description(&self) -> &str {
        "Écrit un rapport Excel (.xlsx) stylé à partir d'une table produite par `junit report`"
    }

    fn signature(&self) -> Signature {
        Signature::build(SimplePluginCommand::name(self))
            .required(
                "path",
                SyntaxShape::Filepath,
                "Chemin du fichier .xlsx à générer",
            )
            .named(
                "project",
                SyntaxShape::String,
                "Nom du projet (affiché en pied de page)",
                None,
            )
            .named(
                "branch",
                SyntaxShape::String,
                "Branche git (affichée en pied de page)",
                None,
            )
            .input_output_type(Type::table(), Type::Nothing)
            .category(Category::Formats)
    }

    fn run(
        &self,
        _plugin: &JunitPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        input: &Value,
    ) -> Result<Value, LabeledError> {
        let span = call.head;
        let path: String = call.req(0)?;
        let project_name: Option<String> = call.get_flag("project")?;
        let branch: Option<String> = call.get_flag("branch")?;

        let cases = value_to_cases(input, span)?;

        if cases.is_empty() {
            return Err(LabeledError::new(
                "Entrée vide : pipez d'abord le résultat de `junit report`",
            )
            .with_label("aucune donnée reçue", span));
        }

        let meta = ReportMeta {
            project_name: project_name.unwrap_or_default(),
            branch: branch.unwrap_or_default(),
            generator_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        write_report(&path, &cases, &meta).map_err(|e| {
            LabeledError::new(format!("Erreur lors de l'écriture du fichier xlsx : {e}"))
                .with_label("erreur d'écriture", span)
        })?;

        Ok(Value::nothing(span))
    }
}

fn value_to_cases(input: &Value, span: Span) -> Result<Vec<TestCase>, LabeledError> {
    let list = input.as_list().map_err(|_| {
        LabeledError::new(
            "Entrée invalide : une table est attendue (utilisez `junit report` en amont du pipeline)",
        )
        .with_label("entrée invalide", span)
    })?;

    let mut cases = Vec::with_capacity(list.len());
    for item in list {
        let record = item.as_record().map_err(|_| {
            LabeledError::new(
                "Chaque ligne doit être un enregistrement avec les colonnes suite/test/status/time",
            )
            .with_label("ligne invalide", span)
        })?;

        let suite = record
            .get("suite")
            .and_then(|v| v.as_str().ok())
            .unwrap_or_default()
            .to_string();
        let name = record
            .get("test")
            .and_then(|v| v.as_str().ok())
            .unwrap_or_default()
            .to_string();
        let status_str = record
            .get("status")
            .and_then(|v| v.as_str().ok())
            .unwrap_or("passed");
        let status = match status_str {
            "failed" => TestStatus::Failed,
            "skipped" => TestStatus::Skipped,
            _ => TestStatus::Passed,
        };
        let time = record
            .get("time")
            .and_then(|v| v.as_float().ok())
            .unwrap_or(0.0) as f32;

        cases.push(TestCase {
            suite,
            name,
            status,
            time,
        });
    }

    Ok(cases)
}

// ---------------------------------------------------------------------

fn main() {
    serve_plugin(&JunitPlugin {}, MsgPackSerializer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nu_plugin_test_support::PluginTest;
    use std::fs;
    use std::sync::Arc;

    const SAMPLE_SUITE_OK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="cnaf.ocda.SampleOkTest" tests="2" failures="0" errors="0" skipped="0" time="1.234">
  <testcase name="shouldDoSomething" classname="cnaf.ocda.SampleOkTest" time="0.5"/>
  <testcase name="shouldDoSomethingElse" classname="cnaf.ocda.SampleOkTest" time="0.734"/>
</testsuite>
"#;

    const SAMPLE_SUITE_MIXED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="cnaf.ocda.SampleMixedTest" tests="3" failures="1" errors="0" skipped="1" time="3.5">
  <testcase name="passes" classname="cnaf.ocda.SampleMixedTest" time="1.5"/>
  <testcase name="fails" classname="cnaf.ocda.SampleMixedTest" time="1.0">
    <failure message="boom">stack trace...</failure>
  </testcase>
  <testcase name="isSkipped" classname="cnaf.ocda.SampleMixedTest" time="0.0">
    <skipped/>
  </testcase>
</testsuite>
"#;

    fn write_sample_reports(dir: &std::path::Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("TEST-SampleOkTest.xml"), SAMPLE_SUITE_OK).unwrap();
        fs::write(dir.join("TEST-SampleMixedTest.xml"), SAMPLE_SUITE_MIXED).unwrap();
    }

    #[test]
    fn junit_report_parses_directory_into_table() {
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_test_{}", std::process::id()));
        write_sample_reports(&tmp);

        let mut test = PluginTest::new("junit", Arc::new(JunitPlugin)).unwrap();
        let result = test
            .eval(&format!("junit report '{}'", tmp.display()))
            .unwrap()
            .into_value(Span::test_data())
            .unwrap();

        let list = result.as_list().unwrap();
        assert_eq!(list.len(), 5, "2 cas OK + 3 cas mixtes = 5 lignes attendues");

        let statuses: Vec<String> = list
            .iter()
            .map(|v| {
                v.as_record()
                    .unwrap()
                    .get("status")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();

        assert_eq!(statuses.iter().filter(|s| *s == "passed").count(), 3);
        assert_eq!(statuses.iter().filter(|s| *s == "failed").count(), 1);
        assert_eq!(statuses.iter().filter(|s| *s == "skipped").count(), 1);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn junit_report_missing_dir_errors_cleanly() {
        let mut test = PluginTest::new("junit", Arc::new(JunitPlugin)).unwrap();
        let err = test.eval("junit report '/chemin/qui/nexiste/pas'");
        assert!(err.is_err(), "un répertoire inexistant doit produire une erreur");
    }

    #[test]
    fn pipeline_report_to_xlsx_writes_a_file() {
        let tmp = std::env::temp_dir().join(format!("nu_plugin_junit_test_xlsx_{}", std::process::id()));
        write_sample_reports(&tmp);
        let out_path = tmp.join("out.xlsx");

        let mut test = PluginTest::new("junit", Arc::new(JunitPlugin)).unwrap();
        test.eval(&format!(
            "junit report '{}' | junit to-xlsx '{}' --project demo --branch main",
            tmp.display(),
            out_path.display()
        ))
        .unwrap();

        assert!(out_path.exists(), "le fichier xlsx doit avoir été créé");
        let size = fs::metadata(&out_path).unwrap().len();
        assert!(size > 0, "le fichier xlsx ne doit pas être vide");

        let _ = fs::remove_dir_all(&tmp);
    }
}
