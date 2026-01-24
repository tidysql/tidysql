use tidysql_config::{Config, Severity};

#[derive(Debug, Clone)]
pub struct LintOption {
    pub name: &'static str,
    pub type_name: &'static str,
    pub default_value: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct LintMetadata {
    pub code: &'static str,
    pub summary: &'static str,
    pub rationale: &'static str,
    pub fixable: bool,
    pub config_example: &'static str,
    pub options: &'static [LintOption],
    pub anti_pattern: &'static str,
    pub best_practices: &'static [&'static str],
    pub notes: &'static [&'static str],
    pub dialects: &'static [&'static str],
    pub level: fn(&Config) -> Severity,
}

fn level_disallow_names(config: &Config) -> Severity {
    config.lints.disallow_names.level
}

fn level_explicit_union(config: &Config) -> Severity {
    config.lints.explicit_union.level
}

fn level_keyword_case(config: &Config) -> Severity {
    config.lints.keyword_case.level
}

const KEYWORD_CASE_OPTIONS: &[LintOption] = &[
    LintOption {
        name: "policy",
        type_name: "string",
        default_value: "consistent",
        description: "One of: consistent, upper, lower, capitalise, pascal, snake, camel",
    },
    LintOption {
        name: "ignore_words",
        type_name: "array<string>",
        default_value: "[]",
        description: "Keywords to ignore (case-insensitive)",
    },
    LintOption {
        name: "ignore_words_regex",
        type_name: "array<string>",
        default_value: "[]",
        description: "Regex patterns for keywords to ignore",
    },
];

const DISALLOW_NAMES_OPTIONS: &[LintOption] = &[
    LintOption {
        name: "names",
        type_name: "array<string>",
        default_value: "[]",
        description: "Identifier names to disallow (case-insensitive)",
    },
    LintOption {
        name: "regexes",
        type_name: "array<string>",
        default_value: "[]",
        description: "Regex patterns for identifiers to disallow",
    },
];

const EXPLICIT_UNION_OPTIONS: &[LintOption] = &[];

const DIALECTS_EXPLICIT_UNION: &[&str] =
    &["ansi", "bigquery", "clickhouse", "databricks", "mysql", "redshift", "snowflake", "trino"];

pub const LINTS: &[LintMetadata] = &[
    LintMetadata {
        code: "keyword_case",
        summary: "Enforces consistent capitalisation of SQL keywords.",
        rationale: "Consistent keyword casing improves readability and reduces visual noise.",
        fixable: true,
        config_example: r#"[lints]
keyword_case = { level = "warn", policy = "upper" }"#,
        options: KEYWORD_CASE_OPTIONS,
        anti_pattern: "SeLeCt 1 from my_table",
        best_practices: &["SELECT 1 FROM my_table", "select 1 from my_table"],
        notes: &[
            "Default level is allow; adding a keyword_case entry without level implies warn.",
            "Policy snake and camel are treated as lowercase for SQL keywords.",
        ],
        dialects: &[],
        level: level_keyword_case,
    },
    LintMetadata {
        code: "explicit_union",
        summary: "Requires UNION to explicitly specify ALL or DISTINCT.",
        rationale: "Explicit set operators reduce ambiguity and make intent clear.",
        fixable: true,
        config_example: r#"[lints]
explicit_union = { level = "warn" }"#,
        options: EXPLICIT_UNION_OPTIONS,
        anti_pattern: "SELECT 1 UNION SELECT 2",
        best_practices: &["SELECT 1 UNION DISTINCT SELECT 2", "SELECT 1 UNION ALL SELECT 2"],
        notes: &["Auto-fix inserts DISTINCT when missing."],
        dialects: DIALECTS_EXPLICIT_UNION,
        level: level_explicit_union,
    },
    LintMetadata {
        code: "disallow_names",
        summary: "Disallows specific identifier names.",
        rationale: "Preventing weak or temporary names keeps schemas clean.",
        fixable: false,
        config_example: r#"[lints]
disallow_names = { level = "warn", names = ["temp", "tmp"], regexes = ["^_"] }"#,
        options: DISALLOW_NAMES_OPTIONS,
        anti_pattern: "SELECT * FROM temp",
        best_practices: &["SELECT * FROM staging_orders"],
        notes: &[
            "Supports shorthand: disallow_names = [\"temp\", \"tmp\"].",
            "Quoted identifiers are unquoted before matching.",
        ],
        dialects: &[],
        level: level_disallow_names,
    },
];

pub fn lint_metadata(code: &str) -> Option<&'static LintMetadata> {
    LINTS.iter().find(|lint| lint.code == code)
}
