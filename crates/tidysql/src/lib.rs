use std::fmt;

use tidysql_config::Dialect;
pub use tidysql_lints::{Diagnostic, Severity};
use tidysql_syntax::{DialectKind, EditError, Fix, ParseError, TextEdit};

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

impl From<ParseError> for FixError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<EditError> for FixError {
    fn from(error: EditError) -> Self {
        Self::Apply(error)
    }
}

impl fmt::Display for FixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixError::Parse(error) => write!(f, "{error}"),
            FixError::Apply(error) => write!(f, "failed to apply fixes: {error}"),
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

pub fn format_with_config(
    source: &str,
    config: &tidysql_config::Config,
) -> Result<String, tidysql_formatter::FormatError> {
    let dialect = config_dialect(config);
    tidysql_formatter::format_with_dialect(source, dialect)
}

pub fn fix_with_config(source: &str, config: &tidysql_config::Config) -> Result<String, FixError> {
    let dialect = config_dialect(config);
    let mut current = source.to_string();

    for phase in [FixPhase::Structural, FixPhase::Style] {
        let tree = tidysql_syntax::parse(&current, dialect)?;
        let diagnostics = tidysql_lints::run(dialect, &tree, config);
        let edits = collect_phase_fixes(&diagnostics, phase);
        if edits.is_empty() {
            continue;
        }

        current = tidysql_syntax::apply_edits(&current, edits)?;
    }

    Ok(current)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FixPhase {
    Structural,
    Style,
}

fn collect_phase_fixes(diagnostics: &[Diagnostic], phase: FixPhase) -> Vec<TextEdit> {
    let mut accepted = Vec::new();

    for fix in diagnostics
        .iter()
        .filter(|diagnostic| fix_phase(diagnostic.code) == phase)
        .filter_map(|diagnostic| diagnostic.fix.as_ref())
    {
        if !fix_is_compatible(fix, &accepted) {
            continue;
        }
        accepted.extend(fix.edits.iter().cloned());
    }

    accepted
}

fn fix_is_compatible(fix: &Fix, accepted: &[TextEdit]) -> bool {
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|edit| (usize::from(edit.range.start()), usize::from(edit.range.end())));

    for pair in edits.windows(2) {
        if edits_overlap(&pair[0], &pair[1]) {
            return false;
        }
    }

    edits
        .iter()
        .all(|candidate| accepted.iter().all(|existing| !edits_overlap(existing, candidate)))
}

fn edits_overlap(left: &TextEdit, right: &TextEdit) -> bool {
    let left_start = usize::from(left.range.start());
    let left_end = usize::from(left.range.end());
    let right_start = usize::from(right.range.start());
    let right_end = usize::from(right.range.end());

    left_start < right_end && right_start < left_end
}

fn fix_phase(code: &str) -> FixPhase {
    match code {
        "count_rows" | "keyword_case" | "not_equal_style" | "order_by_direction" => FixPhase::Style,
        _ => FixPhase::Structural,
    }
}

fn diagnostics_from_parse_error(error: ParseError) -> Vec<Diagnostic> {
    match error {
        ParseError::UnavailableDialect(kind) => vec![Diagnostic::new(
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
            error.span.map_or(0..0, |span| span.source_range()),
        )],
        ParseError::UnparsableRanges(ranges) => ranges
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
        ParseError::ParserPanic(message) => {
            vec![Diagnostic::new(CODE_PANIC, message, Severity::Error, 0..0)]
        }
    }
}

pub fn config_dialect(config: &tidysql_config::Config) -> DialectKind {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_with_config_applies_structural_then_style_fixes() {
        let config = tidysql_config::Config::from_toml_str(
            r#"
[lints]
null_comparison = { level = "warn" }
not_equal_style = { level = "warn", preferred = "angle" }
"#,
        )
        .unwrap();

        let fixed = fix_with_config("SELECT * FROM foo WHERE a != NULL", &config).unwrap();
        assert_eq!(fixed, "SELECT * FROM foo WHERE a IS NOT NULL");
    }
}
