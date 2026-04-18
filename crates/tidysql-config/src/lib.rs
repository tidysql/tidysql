use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::de::{Deserializer, Error as DeError, IntoDeserializer, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use serde_untagged::UntaggedEnumVisitor;

pub const DEFAULT_CONFIG_FILE: &str = "tidysql.toml";

fn expected_values<T>(values: &[T], as_str: impl Fn(&T) -> &'static str) -> String {
    let mut expected = String::new();

    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            expected.push_str(", ");
        }
        expected.push_str(as_str(value));
    }

    expected
}

fn find_named_value<T: Copy>(
    values: &[T],
    normalized: &str,
    as_str: impl Fn(&T) -> &'static str,
) -> Option<T> {
    values.iter().find(|value| normalized == as_str(value)).copied()
}

fn deserialize_regexes<'de, D>(deserializer: D, path: &'static str) -> Result<Vec<Regex>, D::Error>
where
    D: Deserializer<'de>,
{
    struct RegexesVisitor {
        path: &'static str,
    }

    impl<'de> Visitor<'de> for RegexesVisitor {
        type Value = Vec<Regex>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a list of regex patterns")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut compiled = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            let mut index = 0;

            while let Some(pattern) = seq.next_element::<Cow<'de, str>>()? {
                match Regex::new(pattern.as_ref()) {
                    Ok(regex) => compiled.push(regex),
                    Err(error) => {
                        return Err(DeError::custom(format!(
                            "invalid {}[{index}] (`{pattern}`): {error}",
                            self.path
                        )));
                    }
                }
                index += 1;
            }

            Ok(compiled)
        }
    }

    deserializer.deserialize_seq(RegexesVisitor { path })
}

fn serialize_regexes<S>(regexes: &[Regex], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq = serializer.serialize_seq(Some(regexes.len()))?;
    for regex in regexes {
        seq.serialize_element(regex.as_str())?;
    }
    seq.end()
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    #[default]
    Ansi,
    Athena,
    Bigquery,
    Clickhouse,
    Databricks,
    Duckdb,
    Mysql,
    Postgres,
    Redshift,
    Snowflake,
    Sparksql,
    Sqlite,
    Trino,
    Tsql,
}

impl Dialect {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Dialect::Ansi => "ansi",
            Dialect::Athena => "athena",
            Dialect::Bigquery => "bigquery",
            Dialect::Clickhouse => "clickhouse",
            Dialect::Databricks => "databricks",
            Dialect::Duckdb => "duckdb",
            Dialect::Mysql => "mysql",
            Dialect::Postgres => "postgres",
            Dialect::Redshift => "redshift",
            Dialect::Snowflake => "snowflake",
            Dialect::Sparksql => "sparksql",
            Dialect::Sqlite => "sqlite",
            Dialect::Trino => "trino",
            Dialect::Tsql => "tsql",
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Dialect::Ansi => "ANSI",
            Dialect::Athena => "Athena",
            Dialect::Bigquery => "BigQuery",
            Dialect::Clickhouse => "ClickHouse",
            Dialect::Databricks => "Databricks",
            Dialect::Duckdb => "DuckDB",
            Dialect::Mysql => "MySQL",
            Dialect::Postgres => "Postgres",
            Dialect::Redshift => "Redshift",
            Dialect::Snowflake => "Snowflake",
            Dialect::Sparksql => "SparkSQL",
            Dialect::Sqlite => "SQLite",
            Dialect::Trino => "Trino",
            Dialect::Tsql => "TSQL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DialectParseError {
    input: String,
}

impl fmt::Display for DialectParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected = expected_values(DIALECTS, Dialect::as_str);
        write!(f, "invalid dialect '{}', expected one of: {expected}", self.input)
    }
}

impl std::error::Error for DialectParseError {}

impl std::str::FromStr for Dialect {
    type Err = DialectParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.to_ascii_lowercase();
        find_named_value(DIALECTS, &normalized, Dialect::as_str)
            .ok_or_else(|| DialectParseError { input: input.to_string() })
    }
}

pub const DIALECTS: &[Dialect] = &[
    Dialect::Ansi,
    Dialect::Athena,
    Dialect::Bigquery,
    Dialect::Clickhouse,
    Dialect::Databricks,
    Dialect::Duckdb,
    Dialect::Mysql,
    Dialect::Postgres,
    Dialect::Redshift,
    Dialect::Snowflake,
    Dialect::Sparksql,
    Dialect::Sqlite,
    Dialect::Trino,
    Dialect::Tsql,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintName {
    ConsecutiveSemicolons,
    ConstantExpression,
    CountRows,
    DisallowNames,
    DistinctParentheses,
    ElseNull,
    ExplicitUnion,
    IdentifierCharacters,
    KeywordIdentifier,
    KeywordCase,
    NotEqualStyle,
    NullComparison,
    OrderByDirection,
    RequireOrderBy,
    SelfAliasColumn,
    SimpleCase,
    UniqueColumnAlias,
    UniqueTableAlias,
    UnusedCte,
    UnusedTableAlias,
}

impl LintName {
    pub const fn as_str(&self) -> &'static str {
        match self {
            LintName::ConsecutiveSemicolons => "consecutive_semicolons",
            LintName::ConstantExpression => "constant_expression",
            LintName::CountRows => "count_rows",
            LintName::DisallowNames => "disallow_names",
            LintName::DistinctParentheses => "distinct_parentheses",
            LintName::ElseNull => "else_null",
            LintName::ExplicitUnion => "explicit_union",
            LintName::IdentifierCharacters => "identifier_characters",
            LintName::KeywordIdentifier => "keyword_identifier",
            LintName::KeywordCase => "keyword_case",
            LintName::NotEqualStyle => "not_equal_style",
            LintName::NullComparison => "null_comparison",
            LintName::OrderByDirection => "order_by_direction",
            LintName::RequireOrderBy => "require_order_by",
            LintName::SelfAliasColumn => "self_alias_column",
            LintName::SimpleCase => "simple_case",
            LintName::UniqueColumnAlias => "unique_column_alias",
            LintName::UniqueTableAlias => "unique_table_alias",
            LintName::UnusedCte => "unused_cte",
            LintName::UnusedTableAlias => "unused_table_alias",
        }
    }
}

pub const LINTS: &[LintName] = &[
    LintName::ConsecutiveSemicolons,
    LintName::ConstantExpression,
    LintName::CountRows,
    LintName::DisallowNames,
    LintName::DistinctParentheses,
    LintName::ElseNull,
    LintName::ExplicitUnion,
    LintName::IdentifierCharacters,
    LintName::KeywordIdentifier,
    LintName::KeywordCase,
    LintName::NotEqualStyle,
    LintName::NullComparison,
    LintName::OrderByDirection,
    LintName::RequireOrderBy,
    LintName::SelfAliasColumn,
    LintName::SimpleCase,
    LintName::UniqueColumnAlias,
    LintName::UniqueTableAlias,
    LintName::UnusedCte,
    LintName::UnusedTableAlias,
];

#[derive(Debug, Clone)]
pub struct LintNameParseError {
    input: String,
}

impl fmt::Display for LintNameParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected = expected_values(LINTS, LintName::as_str);
        write!(f, "invalid lint '{}', expected one of: {expected}", self.input)
    }
}

impl std::error::Error for LintNameParseError {}

impl std::str::FromStr for LintName {
    type Err = LintNameParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.to_ascii_lowercase().replace('-', "_");
        find_named_value(LINTS, &normalized, LintName::as_str)
            .ok_or_else(|| LintNameParseError { input: input.to_string() })
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Core {
    pub dialect: Dialect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[serde(alias = "deny")]
    Error,
    #[default]
    Warn,
    Info,
    Hint,
    Allow,
}

#[derive(Debug, Clone, Serialize)]
pub struct LintConfig<T> {
    pub level: Severity,
    pub options: T,
}

impl<T: Default> Default for LintConfig<T> {
    fn default() -> Self {
        Self { level: Severity::Warn, options: T::default() }
    }
}

impl<'de, T> Deserialize<'de> for LintConfig<T>
where
    T: Deserialize<'de> + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .expecting("a severity, an options array, or a table with level and options")
            .string(|value| {
                Severity::deserialize(value.into_deserializer())
                    .map(|level| LintConfig { level, options: T::default() })
            })
            .seq(|seq| {
                let options = seq.deserialize()?;
                Ok(LintConfig { level: Severity::Warn, options })
            })
            .map(|map| {
                let table: LintConfigTable<T> = map.deserialize()?;
                Ok(LintConfig { level: table.level.unwrap_or_default(), options: table.options })
            })
            .deserialize(deserializer)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct LintConfigTable<T> {
    #[serde(default)]
    level: Option<Severity>,
    #[serde(flatten)]
    options: T,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExplicitUnionConfig {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapitalisationPolicy {
    #[default]
    Consistent,
    Upper,
    Lower,
    Pascal,
    Capitalise,
    Snake,
    Camel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotEqualStyle {
    #[default]
    Consistent,
    Angle,
    Bang,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountRowsStyle {
    #[default]
    Star,
    One,
    Zero,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeywordCaseConfig {
    pub policy: CapitalisationPolicy,
    pub ignore_words: Vec<String>,
    #[serde(
        deserialize_with = "deserialize_ignore_words_regex",
        serialize_with = "serialize_ignore_words_regex",
        default
    )]
    pub ignore_words_regex: Vec<Regex>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConsecutiveSemicolonsConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstantExpressionConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CountRowsConfig {
    pub preferred: CountRowsStyle,
}

fn deserialize_ignore_words_regex<'de, D>(deserializer: D) -> Result<Vec<Regex>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_regexes(deserializer, "lints.keyword_case.ignore_words_regex")
}

fn serialize_ignore_words_regex<S>(regexes: &[Regex], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serialize_regexes(regexes, serializer)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DisallowNamesConfig {
    pub names: Vec<String>,
    #[serde(serialize_with = "serialize_disallow_name_regexes")]
    pub regexes: Vec<Regex>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DistinctParenthesesConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ElseNullConfig {}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DisallowNamesConfigTable {
    names: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_disallow_name_regexes")]
    regexes: Vec<Regex>,
}

impl<'de> Deserialize<'de> for DisallowNamesConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .expecting("a list of names or a table with names/regexes")
            .seq(|seq| {
                let names: Vec<String> = seq.deserialize()?;
                Ok(Self { names, regexes: Vec::new() })
            })
            .map(|map| {
                let table: DisallowNamesConfigTable = map.deserialize()?;
                Ok(Self { names: table.names, regexes: table.regexes })
            })
            .deserialize(deserializer)
    }
}

fn deserialize_disallow_name_regexes<'de, D>(deserializer: D) -> Result<Vec<Regex>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_regexes(deserializer, "lints.disallow_names.regexes")
}

fn serialize_disallow_name_regexes<S>(regexes: &[Regex], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serialize_regexes(regexes, serializer)
}

macro_rules! define_lints {
    ($( $field:ident : $config:ty => $name:ident ),+ $(,)?) => {
        #[derive(Debug, Clone, Deserialize, Serialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct Lints {
            $(pub $field: LintConfig<$config>,)+
        }

        impl Lints {
            fn warn_defaults() -> Self {
                Self {
                    $($field: LintConfig::default(),)+
                }
            }

            pub fn set_level(&mut self, lint: LintName, level: Severity) {
                match lint {
                    $(LintName::$name => self.$field.level = level,)+
                }
            }
        }
    };
}

define_lints! {
    consecutive_semicolons: ConsecutiveSemicolonsConfig => ConsecutiveSemicolons,
    constant_expression: ConstantExpressionConfig => ConstantExpression,
    count_rows: CountRowsConfig => CountRows,
    disallow_names: DisallowNamesConfig => DisallowNames,
    distinct_parentheses: DistinctParenthesesConfig => DistinctParentheses,
    else_null: ElseNullConfig => ElseNull,
    explicit_union: ExplicitUnionConfig => ExplicitUnion,
    identifier_characters: IdentifierCharactersConfig => IdentifierCharacters,
    keyword_identifier: KeywordIdentifierConfig => KeywordIdentifier,
    keyword_case: KeywordCaseConfig => KeywordCase,
    not_equal_style: NotEqualStyleConfig => NotEqualStyle,
    null_comparison: NullComparisonConfig => NullComparison,
    order_by_direction: OrderByDirectionConfig => OrderByDirection,
    require_order_by: RequireOrderByConfig => RequireOrderBy,
    self_alias_column: SelfAliasColumnConfig => SelfAliasColumn,
    simple_case: SimpleCaseConfig => SimpleCase,
    unique_column_alias: UniqueColumnAliasConfig => UniqueColumnAlias,
    unique_table_alias: UniqueTableAliasConfig => UniqueTableAlias,
    unused_cte: UnusedCteConfig => UnusedCte,
    unused_table_alias: UnusedTableAliasConfig => UnusedTableAlias,
}

impl Default for Lints {
    fn default() -> Self {
        Self {
            keyword_case: LintConfig { level: Severity::Allow, ..LintConfig::default() },
            ..Self::warn_defaults()
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct IdentifierCharactersConfig {
    pub allow_space: bool,
    pub additional_allowed_characters: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeywordIdentifierConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotEqualStyleConfig {
    pub preferred: NotEqualStyle,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NullComparisonConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OrderByDirectionConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RequireOrderByConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SelfAliasColumnConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SimpleCaseConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UniqueColumnAliasConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UniqueTableAliasConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnusedCteConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnusedTableAliasConfig {}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub core: Core,
    pub lints: Lints,
}

#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: std::io::Error },
    Toml { path: Option<PathBuf>, source: Box<toml::de::Error> },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "failed to read config {}: {source}", path.display())
            }
            ConfigError::Toml { path, source, .. } => match path {
                Some(path) => write!(f, "failed to parse config {}: {source}", path.display()),
                None => write!(f, "failed to parse config: {source}"),
            },
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let (path, input) = read_config(path)?;
        parse_config(&input, Some(path))
    }

    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        parse_config(input, None)
    }

    pub fn set_lint_level(&mut self, lint: LintName, level: Severity) {
        self.lints.set_level(lint, level);
    }
}

pub fn read_config(path: impl AsRef<Path>) -> Result<(PathBuf, String), ConfigError> {
    let path = path.as_ref();
    let input = std::fs::read_to_string(path)
        .map_err(|source| ConfigError::Io { path: path.to_path_buf(), source })?;
    Ok((path.to_path_buf(), input))
}

pub fn parse_config(input: &str, path: Option<PathBuf>) -> Result<Config, ConfigError> {
    let config: Config = toml::from_str(input)
        .map_err(|source| ConfigError::Toml { path, source: Box::new(source) })?;
    Ok(config)
}

pub fn load_config(explicit: Option<&Path>, source_path: &Path) -> Result<Config, ConfigError> {
    let path = explicit.map(PathBuf::from).or_else(|| find_config_path(source_path));
    match path {
        Some(path) => {
            let (path, input) = read_config(&path)?;
            parse_config(&input, Some(path))
        }
        None => Ok(Config::default()),
    }
}

pub fn find_config_path(path: &Path) -> Option<PathBuf> {
    let start = if path.is_dir() { path } else { path.parent().unwrap_or_else(|| Path::new(".")) };

    for dir in start.ancestors() {
        let candidate = dir.join(DEFAULT_CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_deny_alias() {
        let config: Config = toml::from_str(
            r#"
[lints]
disallow_names = { level = "deny", names = ["foo"] }
"#,
        )
        .unwrap();
        assert_eq!(config.lints.disallow_names.level, Severity::Error);
    }

    #[test]
    fn severity_error_roundtrip() {
        let config: Config = toml::from_str(
            r#"
[lints]
disallow_names = { level = "error", names = ["bar"] }
"#,
        )
        .unwrap();
        assert_eq!(config.lints.disallow_names.level, Severity::Error);
    }

    #[test]
    fn dialect_from_str_is_case_insensitive() {
        assert_eq!("PoStGrEs".parse::<Dialect>().unwrap(), Dialect::Postgres);
    }

    #[test]
    fn lint_name_from_str_accepts_dashes() {
        assert_eq!("unused-table-alias".parse::<LintName>().unwrap(), LintName::UnusedTableAlias);
    }

    #[test]
    fn keyword_case_regex_error_reports_field_path() {
        let error = Config::from_toml_str(
            r#"
[lints]
keyword_case = { ignore_words_regex = ["("] }
"#,
        )
        .unwrap_err();

        let ConfigError::Toml { source, .. } = error else {
            panic!("expected toml error");
        };

        assert!(
            source.to_string().contains("invalid lints.keyword_case.ignore_words_regex[0] (`(`):")
        );
    }

    #[test]
    fn disallow_names_regex_error_reports_field_path() {
        let error = Config::from_toml_str(
            r#"
[lints]
disallow_names = { regexes = ["("] }
"#,
        )
        .unwrap_err();

        let ConfigError::Toml { source, .. } = error else {
            panic!("expected toml error");
        };

        assert!(source.to_string().contains("invalid lints.disallow_names.regexes[0] (`(`):"));
    }
}
