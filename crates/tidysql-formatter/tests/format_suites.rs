use std::path::Path;

use serde::Deserialize;
use tidysql_config::Format;
use tidysql_formatter::{FormatError, FormatMode};
use tidysql_syntax::{DialectKind, SyntaxKind, WalkEventWithTokens};

#[derive(Deserialize)]
struct FormatSuite {
    #[serde(default, rename = "case")]
    cases: Vec<FormatCase>,
}

#[derive(Deserialize)]
struct FormatCase {
    #[serde(default)]
    name: Option<String>,
    sql: String,
    #[serde(default)]
    config: Format,
    #[serde(default)]
    mode: CaseMode,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    expect_error: Option<ExpectedError>,
    #[serde(default)]
    idempotent: bool,
    #[serde(default)]
    preserve_structure: bool,
    #[serde(default)]
    all_comments_formatted: bool,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CaseMode {
    #[default]
    Pragmatic,
    Strict,
}

#[derive(Deserialize)]
struct ExpectedError {
    kind: ExpectedErrorKind,
    #[serde(default)]
    syntax_kind: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExpectedErrorKind {
    Parse,
    Unsupported,
}

fn run_case(path: &Path, input: String) -> datatest_stable::Result<()> {
    let suite: FormatSuite = toml::from_str(&input)?;

    if suite.cases.is_empty() {
        return Err(format!("no cases found in {}", path.display()).into());
    }

    for (case_index, case) in suite.cases.iter().enumerate() {
        run_single_case(path, case_index, case)?;
    }

    Ok(())
}

fn run_single_case(
    path: &Path,
    case_index: usize,
    case: &FormatCase,
) -> datatest_stable::Result<()> {
    let label = case_label(case, case_index);
    let result = tidysql_formatter::format_with_config_and_mode(
        &case.sql,
        DialectKind::Ansi,
        &case.config,
        format_mode(case.mode),
    );

    match (&case.expected, &case.expect_error, result) {
        (Some(expected), None, Ok(actual)) => {
            assert_eq!(&actual, expected, "formatted SQL mismatch ({label}) in {}", path.display());

            if case.idempotent {
                let second = tidysql_formatter::format_with_config_and_mode(
                    &actual,
                    DialectKind::Ansi,
                    &case.config,
                    format_mode(case.mode),
                )
                .map_err(|error| format!("{label}: second format failed: {error}"))?;
                assert_eq!(second, actual, "formatting should be idempotent ({label})");
            }

            if case.preserve_structure {
                assert_eq!(
                    structure_without_trivia(&actual),
                    structure_without_trivia(&case.sql),
                    "formatting should preserve parsed token structure ({label})",
                );
            }

            if case.all_comments_formatted {
                assert_eq!(
                    comments_in_order(&actual),
                    comments_in_order(&case.sql),
                    "formatting should preserve every comment ({label})",
                );
            }
        }
        (None, Some(expected), Err(actual)) => assert_expected_error(&label, expected, actual),
        (Some(_), None, Err(error)) => {
            return Err(format!("{label}: expected formatted SQL, got error: {error}").into());
        }
        (None, Some(_), Ok(actual)) => {
            return Err(format!("{label}: expected error, got formatted SQL: {actual:?}").into());
        }
        (Some(_), Some(_), _) => {
            return Err(format!("{label}: case cannot set both expected and expect_error").into());
        }
        (None, None, _) => {
            return Err(format!("{label}: case must set expected or expect_error").into());
        }
    }

    Ok(())
}

fn assert_expected_error(label: &str, expected: &ExpectedError, actual: FormatError) {
    match (&expected.kind, actual) {
        (ExpectedErrorKind::Parse, FormatError::Parse(_)) => {}
        (ExpectedErrorKind::Unsupported, FormatError::UnsupportedSyntax { kind, .. }) => {
            if let Some(expected_kind) = &expected.syntax_kind {
                assert_eq!(
                    format!("{kind:?}"),
                    *expected_kind,
                    "unsupported syntax kind mismatch ({label})",
                );
            }
        }
        (ExpectedErrorKind::Parse, error) | (ExpectedErrorKind::Unsupported, error) => {
            panic!("unexpected error ({label}): {error:?}");
        }
    }
}

fn format_mode(mode: CaseMode) -> FormatMode {
    match mode {
        CaseMode::Pragmatic => FormatMode::Pragmatic,
        CaseMode::Strict => FormatMode::Strict,
    }
}

fn structure_without_trivia(sql: &str) -> Vec<(SyntaxKind, String)> {
    let tree = tidysql_syntax::parse(sql, DialectKind::Ansi).unwrap();
    tree.root()
        .preorder_with_tokens()
        .filter_map(|event| match event {
            WalkEventWithTokens::Token(token) => Some(token),
            WalkEventWithTokens::EnterNode(_) | WalkEventWithTokens::LeaveNode(_) => None,
        })
        .filter(|token| !is_layout_token(token.kind()) && token.kind() != SyntaxKind::EndOfFile)
        .map(|token| {
            let text = if matches!(token.kind(), SyntaxKind::Keyword | SyntaxKind::BinaryOperator) {
                token.text().to_ascii_lowercase()
            } else {
                token.text().to_string()
            };
            (token.kind(), text)
        })
        .collect()
}

fn comments_in_order(sql: &str) -> Vec<String> {
    let tree = tidysql_syntax::parse(sql, DialectKind::Ansi).unwrap();
    tree.root()
        .preorder_with_tokens()
        .filter_map(|event| match event {
            WalkEventWithTokens::Token(token) => Some(token),
            WalkEventWithTokens::EnterNode(_) | WalkEventWithTokens::LeaveNode(_) => None,
        })
        .flat_map(|token| token.leading_trivia().chain(token.trailing_trivia()).collect::<Vec<_>>())
        .filter(|token| is_comment_token(token.kind()))
        .map(|token| token.text().trim_end().to_string())
        .collect()
}

fn is_layout_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Whitespace
            | SyntaxKind::Newline
            | SyntaxKind::Indent
            | SyntaxKind::Dedent
            | SyntaxKind::Implicit
    )
}

fn is_comment_token(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::InlineComment | SyntaxKind::BlockComment | SyntaxKind::Comment)
}

fn case_label(case: &FormatCase, case_index: usize) -> String {
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
