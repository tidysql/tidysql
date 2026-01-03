use std::ops::Range;

use tidysql_config::Dialect;
use tidysql_syntax::{DialectKind, ParseError, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub severity: Severity,
    pub range: Range<usize>,
}

pub fn check_with_config(source: &str, config: &tidysql_config::Config) -> Vec<Diagnostic> {
    let dialect = config_dialect(config);
    check_with_dialect(source, dialect)
}

fn check_with_dialect(source: &str, dialect: DialectKind) -> Vec<Diagnostic> {
    match tidysql_syntax::parse(source, dialect) {
        Ok(_) => Vec::new(),
        Err(error) => diagnostics_from_parse_error(error),
    }
}

pub fn format_with_config(source: &str, config: &tidysql_config::Config) -> String {
    let dialect = config_dialect(config);
    tidysql_formatter::format_with_dialect(source, dialect)
}

fn diagnostics_from_parse_error(error: ParseError) -> Vec<Diagnostic> {
    match error {
        ParseError::UnknownDialect(kind) => vec![Diagnostic {
            message: format!("Dialect not available: {kind:?}"),
            severity: Severity::Error,
            range: 0..0,
        }],
        ParseError::Lex(errors) => errors
            .into_iter()
            .map(|error| Diagnostic {
                message: error.message,
                severity: Severity::Error,
                range: error.span.source_range(),
            })
            .collect(),
        ParseError::Parse(error) => vec![Diagnostic {
            message: error.description,
            severity: Severity::Error,
            range: error.span.map(|span| span.source_range()).unwrap_or(0..0),
        }],
        ParseError::Unparsable(ranges) => ranges
            .into_iter()
            .map(|range| Diagnostic {
                message: "Unparsable section.".to_string(),
                severity: Severity::Error,
                range: text_range_to_range(range),
            })
            .collect(),
        ParseError::Panic(message) => {
            vec![Diagnostic { message, severity: Severity::Error, range: 0..0 }]
        }
    }
}

fn text_range_to_range(range: TextRange) -> Range<usize> {
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    start..end
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
