use std::path::Path;

use serde::Deserialize;
use tidysql_config::{Config, Dialect};
use tidysql_lints::Severity;
use tidysql_syntax::DialectKind;

#[derive(Deserialize)]
struct LintSuite {
    #[serde(default, rename = "case")]
    cases: Vec<LintCase>,
}

#[derive(Deserialize)]
struct LintCase {
    #[serde(default)]
    name: Option<String>,
    sql: String,
    #[serde(default)]
    config: Config,
    #[serde(default)]
    expect: Vec<ExpectedDiagnostic>,
}

#[derive(Deserialize)]
struct ExpectedDiagnostic {
    code: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    severity: Option<String>,
}

fn run_case(path: &Path, input: String) -> datatest_stable::Result<()> {
    let suite: LintSuite = toml::from_str(&input)?;

    if suite.cases.is_empty() {
        return Err(format!("no cases found in {}", path.display()).into());
    }

    for (case_index, case) in suite.cases.iter().enumerate() {
        run_single_case(path, case_index, case)?;
    }

    Ok(())
}

fn run_single_case(path: &Path, case_index: usize, case: &LintCase) -> datatest_stable::Result<()> {
    let label = case_label(case, case_index);
    let dialect = config_dialect(&case.config);
    let tree =
        tidysql_syntax::parse(&case.sql, dialect).map_err(|error| format!("{label}: {error}"))?;
    let diagnostics = tidysql_lints::run(&case.sql, dialect, &tree, &case.config);

    if diagnostics.len() != case.expect.len() {
        return Err(format!(
            "{label}: expected {} diagnostics, got {}",
            case.expect.len(),
            diagnostics.len()
        )
        .into());
    }

    for (index, (actual, expected)) in diagnostics.iter().zip(case.expect.iter()).enumerate() {
        assert_eq!(
            actual.code,
            expected.code,
            "code mismatch at #{index} ({label}) in {}",
            path.display(),
        );

        if let Some(message) = &expected.message {
            assert_eq!(
                &actual.message,
                message,
                "message mismatch at #{index} ({label}) in {}",
                path.display(),
            );
        }

        if let Some(severity) = &expected.severity {
            let expected = severity.to_ascii_lowercase();
            assert_eq!(
                severity_label(actual.severity),
                expected.as_str(),
                "severity mismatch at #{index} ({label}) in {}",
                path.display(),
            );
        }
    }

    Ok(())
}

fn config_dialect(config: &Config) -> DialectKind {
    match config.core.dialect {
        Dialect::Ansi => DialectKind::Ansi,
        Dialect::Athena => DialectKind::Athena,
        Dialect::Bigquery => DialectKind::Bigquery,
        Dialect::Clickhouse => DialectKind::Clickhouse,
        Dialect::Databricks => DialectKind::Databricks,
        Dialect::Duckdb => DialectKind::Duckdb,
        Dialect::Mysql => DialectKind::Mysql,
        Dialect::Postgres => DialectKind::Postgres,
        Dialect::Redshift => DialectKind::Redshift,
        Dialect::Snowflake => DialectKind::Snowflake,
        Dialect::Sparksql => DialectKind::Sparksql,
        Dialect::Sqlite => DialectKind::Sqlite,
        Dialect::Trino => DialectKind::Trino,
        Dialect::Tsql => DialectKind::Tsql,
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
        Severity::Allow => "allow",
    }
}

fn case_label(case: &LintCase, case_index: usize) -> String {
    match &case.name {
        Some(name) => format!("{name} (#{case_index})"),
        None => format!("case #{case_index}"),
    }
}

datatest_stable::harness! {
    {
        test = run_case,
        root = concat!(env!("CARGO_MANIFEST_DIR"), "/tests"),
        pattern = r"^.*\.toml$",
    },
}
