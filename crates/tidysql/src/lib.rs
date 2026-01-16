use std::fmt;

use tidysql_config::Dialect;
pub use tidysql_lints::{Diagnostic, Severity};
use tidysql_syntax::{DialectKind, EditError, ParseError, TextEdit};

const CODE_UNKNOWN_DIALECT: &str = "unknown_dialect";
const CODE_LEX_ERROR: &str = "lex_error";
const CODE_PARSE_ERROR: &str = "parse_error";
const CODE_UNPARSABLE: &str = "unparsable";
const CODE_PANIC: &str = "parser_panic";

#[derive(Debug)]
pub enum FixError {
    Parse(ParseError),
    Apply(EditError),
}

impl fmt::Display for FixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixError::Parse(error) => write!(f, "{error}"),
            FixError::Apply(error) => write!(f, "failed to apply fixes: {error:?}"),
        }
    }
}

impl std::error::Error for FixError {}

pub fn check_with_config(source: &str, config: &tidysql_config::Config) -> Vec<Diagnostic> {
    let dialect = config_dialect(config);
    check_with_dialect(source, dialect, config)
}

fn check_with_dialect(
    source: &str,
    dialect: DialectKind,
    config: &tidysql_config::Config,
) -> Vec<Diagnostic> {
    match tidysql_syntax::parse(source, dialect) {
        Ok(tree) => tidysql_lints::run(dialect, &tree, config),
        Err(error) => diagnostics_from_parse_error(error),
    }
}

pub fn format_with_config(source: &str, config: &tidysql_config::Config) -> String {
    let dialect = config_dialect(config);
    tidysql_formatter::format_with_dialect(source, dialect)
}

pub fn fix_with_config(source: &str, config: &tidysql_config::Config) -> Result<String, FixError> {
    let dialect = config_dialect(config);
    let tree = tidysql_syntax::parse(source, dialect).map_err(FixError::Parse)?;
    let diagnostics = tidysql_lints::run(dialect, &tree, config);
    let edits = collect_fixes(&diagnostics);

    if edits.is_empty() {
        return Ok(source.to_string());
    }

    tidysql_syntax::apply_edits(source, edits).map_err(FixError::Apply)
}

fn collect_fixes(diagnostics: &[Diagnostic]) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    for diagnostic in diagnostics {
        if let Some(fix) = &diagnostic.fix {
            edits.extend(fix.edits.iter().cloned());
        }
    }

    edits
}

fn diagnostics_from_parse_error(error: ParseError) -> Vec<Diagnostic> {
    match error {
        ParseError::UnknownDialect(kind) => vec![Diagnostic::new(
            CODE_UNKNOWN_DIALECT,
            format!("Dialect not available: {kind:?}"),
            Severity::Error,
            0..0,
        )],
        ParseError::Lex(errors) => errors
            .into_iter()
            .map(|error| {
                Diagnostic::new(
                    CODE_LEX_ERROR,
                    error.message,
                    Severity::Error,
                    error.span.source_range(),
                )
            })
            .collect(),
        ParseError::Parse(error) => vec![Diagnostic::new(
            CODE_PARSE_ERROR,
            error.description,
            Severity::Error,
            error.span.map(|span| span.source_range()).unwrap_or(0..0),
        )],
        ParseError::Unparsable(ranges) => ranges
            .into_iter()
            .map(|range| {
                Diagnostic::from_text_range(
                    CODE_UNPARSABLE,
                    "Unparsable section.",
                    Severity::Error,
                    range,
                )
            })
            .collect(),
        ParseError::Panic(message) => {
            vec![Diagnostic::new(CODE_PANIC, message, Severity::Error, 0..0)]
        }
    }
}

fn config_dialect(config: &tidysql_config::Config) -> DialectKind {
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
