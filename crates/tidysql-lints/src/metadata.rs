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

const DIALECTS_EXPLICIT_UNION: &[&str] =
    &["ansi", "bigquery", "clickhouse", "databricks", "mysql", "redshift", "snowflake", "trino"];

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

const COUNT_ROWS_OPTIONS: &[LintOption] = &[LintOption {
    name: "preferred",
    type_name: "string",
    default_value: "star",
    description: "Preferred row-counting style: star, one, or zero",
}];

const NOT_EQUAL_STYLE_OPTIONS: &[LintOption] = &[LintOption {
    name: "preferred",
    type_name: "string",
    default_value: "consistent",
    description: "Preferred not-equal operator: consistent, angle, or bang",
}];

const IDENTIFIER_CHAR_OPTIONS: &[LintOption] = &[
    LintOption {
        name: "allow_space",
        type_name: "boolean",
        default_value: "false",
        description: "Allow spaces inside identifiers",
    },
    LintOption {
        name: "additional_allowed_characters",
        type_name: "string",
        default_value: "\"\"",
        description: "Extra non-alphanumeric characters to allow",
    },
];

fn level_consecutive_semicolons(config: &Config) -> Severity {
    config.lints.consecutive_semicolons.level
}

fn level_constant_expression(config: &Config) -> Severity {
    config.lints.constant_expression.level
}

fn level_count_rows(config: &Config) -> Severity {
    config.lints.count_rows.level
}

fn level_disallow_names(config: &Config) -> Severity {
    config.lints.disallow_names.level
}

fn level_distinct_parentheses(config: &Config) -> Severity {
    config.lints.distinct_parentheses.level
}

fn level_else_null(config: &Config) -> Severity {
    config.lints.else_null.level
}

fn level_explicit_union(config: &Config) -> Severity {
    config.lints.explicit_union.level
}

fn level_identifier_characters(config: &Config) -> Severity {
    config.lints.identifier_characters.level
}

fn level_keyword_identifier(config: &Config) -> Severity {
    config.lints.keyword_identifier.level
}

fn level_keyword_case(config: &Config) -> Severity {
    config.lints.keyword_case.level
}

fn level_not_equal_style(config: &Config) -> Severity {
    config.lints.not_equal_style.level
}

fn level_null_comparison(config: &Config) -> Severity {
    config.lints.null_comparison.level
}

fn level_order_by_direction(config: &Config) -> Severity {
    config.lints.order_by_direction.level
}

fn level_require_order_by(config: &Config) -> Severity {
    config.lints.require_order_by.level
}

fn level_self_alias_column(config: &Config) -> Severity {
    config.lints.self_alias_column.level
}

fn level_simple_case(config: &Config) -> Severity {
    config.lints.simple_case.level
}

fn level_unique_column_alias(config: &Config) -> Severity {
    config.lints.unique_column_alias.level
}

fn level_unique_table_alias(config: &Config) -> Severity {
    config.lints.unique_table_alias.level
}

fn level_unused_cte(config: &Config) -> Severity {
    config.lints.unused_cte.level
}

fn level_unused_table_alias(config: &Config) -> Severity {
    config.lints.unused_table_alias.level
}

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
        options: &[],
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
    LintMetadata {
        code: "consecutive_semicolons",
        summary: "Flags duplicate semicolons that create empty statements.",
        rationale: "Duplicate terminators usually indicate accidental empty statements.",
        fixable: true,
        config_example: r#"[lints]
consecutive_semicolons = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT 1;;",
        best_practices: &["SELECT 1;"],
        notes: &[],
        dialects: &[],
        level: level_consecutive_semicolons,
    },
    LintMetadata {
        code: "constant_expression",
        summary: "Flags constant or self-comparing expressions.",
        rationale: "Expressions that are always true or compare a value to itself are often \
                    mistakes.",
        fixable: false,
        config_example: r#"[lints]
constant_expression = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT * FROM foo WHERE foo.id = foo.id",
        best_practices: &["SELECT * FROM foo WHERE foo.id > 10"],
        notes: &[],
        dialects: &[],
        level: level_constant_expression,
    },
    LintMetadata {
        code: "count_rows",
        summary: "Enforces a consistent COUNT() style for row counts.",
        rationale: "A single row-count convention keeps aggregation code uniform and easier to \
                    scan.",
        fixable: true,
        config_example: r#"[lints]
count_rows = { level = "warn", preferred = "star" }"#,
        options: COUNT_ROWS_OPTIONS,
        anti_pattern: "SELECT COUNT(1) FROM foo",
        best_practices: &["SELECT COUNT(*) FROM foo"],
        notes: &[],
        dialects: &[],
        level: level_count_rows,
    },
    LintMetadata {
        code: "distinct_parentheses",
        summary: "Disallows DISTINCT used like a function call.",
        rationale: "DISTINCT applies to the whole select list, so parentheses around the first \
                    item are misleading.",
        fixable: true,
        config_example: r#"[lints]
distinct_parentheses = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT DISTINCT(a), b FROM foo",
        best_practices: &["SELECT DISTINCT a, b FROM foo"],
        notes: &[],
        dialects: &[],
        level: level_distinct_parentheses,
    },
    LintMetadata {
        code: "else_null",
        summary: "Removes redundant ELSE NULL branches from CASE expressions.",
        rationale: "CASE expressions already default to NULL when no ELSE branch is provided.",
        fixable: true,
        config_example: r#"[lints]
else_null = { level = "warn" }"#,
        options: &[],
        anti_pattern: "CASE WHEN flag THEN 1 ELSE NULL END",
        best_practices: &["CASE WHEN flag THEN 1 END"],
        notes: &[],
        dialects: &[],
        level: level_else_null,
    },
    LintMetadata {
        code: "identifier_characters",
        summary: "Flags identifiers with special characters.",
        rationale: "Simple identifiers are easier to quote correctly and are more portable across \
                    dialects.",
        fixable: false,
        config_example: r#"[lints]
identifier_characters = { level = "warn", allow_space = false, additional_allowed_characters = "" }"#,
        options: IDENTIFIER_CHAR_OPTIONS,
        anti_pattern: r#"SELECT a AS "has space" FROM foo"#,
        best_practices: &["SELECT a AS has_space FROM foo"],
        notes: &[],
        dialects: &[],
        level: level_identifier_characters,
    },
    LintMetadata {
        code: "keyword_identifier",
        summary: "Avoids SQL keywords as identifiers.",
        rationale: "Keyword-shaped identifiers are valid in some contexts but remain confusing to \
                    readers.",
        fixable: false,
        config_example: r#"[lints]
keyword_identifier = { level = "warn" }"#,
        options: &[],
        anti_pattern: r#"SELECT a AS "select" FROM foo"#,
        best_practices: &["SELECT a AS selected_value FROM foo"],
        notes: &[],
        dialects: &[],
        level: level_keyword_identifier,
    },
    LintMetadata {
        code: "not_equal_style",
        summary: "Enforces a consistent not-equal operator style.",
        rationale: "Using one not-equal form throughout a codebase reduces visual churn.",
        fixable: true,
        config_example: r#"[lints]
not_equal_style = { level = "warn", preferred = "angle" }"#,
        options: NOT_EQUAL_STYLE_OPTIONS,
        anti_pattern: "SELECT * FROM foo WHERE a != b",
        best_practices: &["SELECT * FROM foo WHERE a <> b"],
        notes: &["The consistent mode infers the dominant style in the file."],
        dialects: &[],
        level: level_not_equal_style,
    },
    LintMetadata {
        code: "null_comparison",
        summary: "Requires IS or IS NOT for NULL checks.",
        rationale: "Using equality operators with NULL is misleading and often incorrect.",
        fixable: true,
        config_example: r#"[lints]
null_comparison = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT * FROM foo WHERE x = NULL",
        best_practices: &[
            "SELECT * FROM foo WHERE x IS NULL",
            "SELECT * FROM foo WHERE x IS NOT NULL",
        ],
        notes: &[],
        dialects: &[],
        level: level_null_comparison,
    },
    LintMetadata {
        code: "order_by_direction",
        summary: "Requires consistent ASC/DESC modifiers across ORDER BY items.",
        rationale: "When some ORDER BY items include a direction and others do not, intent is \
                    harder to read.",
        fixable: true,
        config_example: r#"[lints]
order_by_direction = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT a, b FROM foo ORDER BY a, b DESC",
        best_practices: &["SELECT a, b FROM foo ORDER BY a ASC, b DESC"],
        notes: &[],
        dialects: &[],
        level: level_order_by_direction,
    },
    LintMetadata {
        code: "require_order_by",
        summary: "Flags LIMIT or OFFSET without ORDER BY.",
        rationale: "Top-N queries without an explicit ordering are typically non-deterministic.",
        fixable: false,
        config_example: r#"[lints]
require_order_by = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT * FROM foo LIMIT 10",
        best_practices: &["SELECT * FROM foo ORDER BY id LIMIT 10"],
        notes: &[],
        dialects: &[],
        level: level_require_order_by,
    },
    LintMetadata {
        code: "self_alias_column",
        summary: "Removes column aliases that repeat the source name.",
        rationale: "Self-aliases add noise without changing query semantics.",
        fixable: true,
        config_example: r#"[lints]
self_alias_column = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT customer_id AS customer_id FROM orders",
        best_practices: &["SELECT customer_id FROM orders"],
        notes: &["The current implementation only flags exact same-name aliases."],
        dialects: &[],
        level: level_self_alias_column,
    },
    LintMetadata {
        code: "simple_case",
        summary: "Simplifies CASE expressions that only fill NULL values.",
        rationale: "NULL-filling CASE expressions are clearer as COALESCE calls.",
        fixable: true,
        config_example: r#"[lints]
simple_case = { level = "warn" }"#,
        options: &[],
        anti_pattern: "CASE WHEN x IS NULL THEN 0 ELSE x END",
        best_practices: &["COALESCE(x, 0)"],
        notes: &["The current implementation targets the common CASE ... IS NULL ... ELSE \
                  same-value pattern."],
        dialects: &[],
        level: level_simple_case,
    },
    LintMetadata {
        code: "unique_column_alias",
        summary: "Requires column aliases to be unique within a SELECT clause.",
        rationale: "Duplicate output names are confusing and often accidental.",
        fixable: false,
        config_example: r#"[lints]
unique_column_alias = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT a AS value, b AS value FROM foo",
        best_practices: &["SELECT a AS left_value, b AS right_value FROM foo"],
        notes: &[],
        dialects: &[],
        level: level_unique_column_alias,
    },
    LintMetadata {
        code: "unique_table_alias",
        summary: "Requires table aliases to be unique within a statement.",
        rationale: "Reused table aliases make column qualification ambiguous to readers.",
        fixable: false,
        config_example: r#"[lints]
unique_table_alias = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT t.id FROM foo AS t JOIN bar AS t ON t.id = t.id",
        best_practices: &["SELECT f.id FROM foo AS f JOIN bar AS b ON f.id = b.id"],
        notes: &[],
        dialects: &[],
        level: level_unique_table_alias,
    },
    LintMetadata {
        code: "unused_cte",
        summary: "Flags CTEs that are defined but never referenced.",
        rationale: "Unused CTEs add maintenance cost and often reflect incomplete refactors.",
        fixable: false,
        config_example: r#"[lints]
unused_cte = { level = "warn" }"#,
        options: &[],
        anti_pattern: "WITH cte AS (SELECT 1), dead AS (SELECT 2) SELECT * FROM cte",
        best_practices: &["WITH cte AS (SELECT 1) SELECT * FROM cte"],
        notes: &[],
        dialects: &[],
        level: level_unused_cte,
    },
    LintMetadata {
        code: "unused_table_alias",
        summary: "Removes table aliases that are never referenced.",
        rationale: "Unused aliases make FROM and JOIN clauses noisier without adding clarity.",
        fixable: true,
        config_example: r#"[lints]
unused_table_alias = { level = "warn" }"#,
        options: &[],
        anti_pattern: "SELECT a FROM foo AS f",
        best_practices: &["SELECT a FROM foo"],
        notes: &["The current implementation removes only aliases that are not referenced \
                  anywhere else in the statement."],
        dialects: &[],
        level: level_unused_table_alias,
    },
];

pub fn lint_metadata(code: &str) -> Option<&'static LintMetadata> {
    LINTS.iter().find(|lint| lint.code == code)
}
