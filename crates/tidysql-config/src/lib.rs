use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use serde::de::{Deserializer, Error as DeError, IntoDeserializer, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Serialize, Serializer};
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

#[derive(Debug, Clone)]
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

impl<T> Serialize for LintConfig<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = toml::Value::try_from(&self.options).map_err(serde::ser::Error::custom)?;
        let toml::Value::Table(options) = value else {
            return Err(serde::ser::Error::custom("lint options must serialize as a table"));
        };

        let mut map = serializer.serialize_map(Some(options.len() + 1))?;
        map.serialize_entry("level", &self.level)?;
        for (key, value) in options {
            map.serialize_entry(&key, &value)?;
        }
        map.end()
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesConfig {
    pub include: Vec<String>,
    pub extend_include: Vec<String>,
    pub exclude: Vec<String>,
    pub extend_exclude: Vec<String>,
    pub respect_gitignore: bool,
    pub force_exclude: bool,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self {
            include: vec!["**/*.sql".to_string()],
            extend_include: Vec::new(),
            exclude: DEFAULT_EXCLUDES.iter().map(|pattern| (*pattern).to_string()).collect(),
            extend_exclude: Vec::new(),
            respect_gitignore: true,
            force_exclude: false,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub extend: Option<PathBuf>,
    pub files: FilesConfig,
    pub core: Core,
    pub lints: Lints,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawConfigFile {
    extend: Option<PathBuf>,
    files: RawFilesConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawFilesConfig {
    include: Option<Vec<String>>,
    extend_include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    extend_exclude: Option<Vec<String>>,
    respect_gitignore: Option<bool>,
    force_exclude: Option<bool>,
}

#[derive(Debug)]
pub struct ResolvedConfig {
    config: Config,
    files: FilesConfig,
    config_path: Option<PathBuf>,
    base_dir: PathBuf,
    include_matchers: Vec<CompiledGlobMatcher>,
    exclude_matchers: Vec<CompiledGlobMatcher>,
}

#[derive(Debug, Clone)]
struct CompiledGlobMatcher {
    relative_root: Option<PathBuf>,
    relative_set: GlobSet,
    has_relative_patterns: bool,
    absolute_set: GlobSet,
    has_absolute_patterns: bool,
}

#[derive(Default)]
struct ResolverState {
    nearest_configs: HashMap<PathBuf, Option<PathBuf>>,
    configs: HashMap<PathBuf, Arc<ResolvedConfig>>,
}

#[derive(Default)]
pub struct ConfigResolver {
    state: RwLock<ResolverState>,
    default_config: OnceLock<Arc<ResolvedConfig>>,
}

#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: std::io::Error },
    Toml { path: Option<PathBuf>, source: Box<toml::de::Error> },
    ExtendCycle { path: PathBuf },
    InvalidExtend { path: PathBuf, message: String },
    Glob { path: Option<PathBuf>, pattern: String, source: globset::Error },
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
            ConfigError::ExtendCycle { path } => {
                write!(f, "cyclic config inheritance detected at {}", path.display())
            }
            ConfigError::InvalidExtend { path, message } => {
                write!(f, "invalid extend in config {}: {message}", path.display())
            }
            ConfigError::Glob { path, pattern, source } => match path {
                Some(path) => write!(
                    f,
                    "failed to compile file pattern `{pattern}` in config {}: {source}",
                    path.display()
                ),
                None => write!(f, "failed to compile file pattern `{pattern}`: {source}"),
            },
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let resolved = ConfigResolver::new().resolve_explicit(path.as_ref())?;
        Ok(resolved.config().clone())
    }

    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        toml::from_str(input)
            .map_err(|source| ConfigError::Toml { path: None, source: Box::new(source) })
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
    let config: ConfigFile = toml::from_str(input)
        .map_err(|source| ConfigError::Toml { path, source: Box::new(source) })?;
    Ok(config.config())
}

pub fn load_config(explicit: Option<&Path>, source_path: &Path) -> Result<Config, ConfigError> {
    let resolver = ConfigResolver::new();
    let resolved = match explicit {
        Some(path) => resolver.resolve_explicit(path)?,
        None => resolver.resolve(source_path)?,
    };
    Ok(resolved.config().clone())
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

const DEFAULT_EXCLUDES: &[&str] = &[
    ".git/**",
    "**/.git/**",
    "node_modules/**",
    "**/node_modules/**",
    "dist/**",
    "**/dist/**",
    "build/**",
    "**/build/**",
    "target/**",
    "**/target/**",
    ".venv/**",
    "**/.venv/**",
    ".idea/**",
    "**/.idea/**",
    ".vscode/**",
    "**/.vscode/**",
];

impl ConfigFile {
    fn config(self) -> Config {
        Config { core: self.core, lints: self.lints }
    }
}

impl ResolvedConfig {
    fn new(
        config: Config,
        files: FilesConfig,
        config_path: Option<PathBuf>,
        include_matchers: Vec<CompiledGlobMatcher>,
        exclude_matchers: Vec<CompiledGlobMatcher>,
    ) -> Self {
        let base_dir = config_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(default_base_dir);

        Self { config, files, config_path, base_dir, include_matchers, exclude_matchers }
    }

    fn default() -> Result<Self, ConfigError> {
        let files = FilesConfig::default();
        let include_matchers = vec![CompiledGlobMatcher::compile(&files.include, None, None)?];
        let exclude_matchers = vec![CompiledGlobMatcher::compile(&files.exclude, None, None)?];

        Ok(Self::new(Config::default(), files, None, include_matchers, exclude_matchers))
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    pub fn files(&self) -> &FilesConfig {
        &self.files
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn matches_discovered_file(&self, path: &Path) -> bool {
        !self.exclude_matchers.iter().any(|matcher| matcher.is_match(path))
            && self.include_matchers.iter().any(|matcher| matcher.is_match(path))
    }

    pub fn matches_explicit_file(&self, path: &Path) -> bool {
        if !self.files.force_exclude {
            return true;
        }

        self.matches_discovered_file(path)
    }

    pub fn excludes_directory(&self, path: &Path) -> bool {
        self.exclude_matchers.iter().any(|matcher| matcher.matches_path_or_descendant(path))
    }

    pub fn relative_path_string(&self, path: &Path) -> String {
        normalize_absolute_path_for_matcher(path)
    }
}

impl CompiledGlobMatcher {
    fn compile(
        patterns: &[String],
        base_dir: Option<&Path>,
        path: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let mut relative_builder = GlobSetBuilder::new();
        let mut absolute_builder = GlobSetBuilder::new();
        let mut has_relative_patterns = false;
        let mut has_absolute_patterns = false;

        for pattern in patterns {
            let pattern_path = Path::new(pattern);
            if pattern_path.is_absolute() {
                let glob = Glob::new(&normalize_absolute_path_for_matcher(pattern_path)).map_err(
                    |source| ConfigError::Glob {
                        path: path.clone(),
                        pattern: pattern.clone(),
                        source,
                    },
                )?;
                absolute_builder.add(glob);
                has_absolute_patterns = true;
            } else {
                let glob = Glob::new(pattern).map_err(|source| ConfigError::Glob {
                    path: path.clone(),
                    pattern: pattern.clone(),
                    source,
                })?;
                relative_builder.add(glob);
                has_relative_patterns = true;
            }
        }

        let relative_set = relative_builder.build().map_err(|source| ConfigError::Glob {
            path,
            pattern: "<globset>".to_string(),
            source,
        })?;
        let absolute_set = absolute_builder.build().map_err(|source| ConfigError::Glob {
            path: None,
            pattern: "<globset>".to_string(),
            source,
        })?;

        Ok(Self {
            relative_root: base_dir.map(normalize_path),
            relative_set,
            has_relative_patterns,
            absolute_set,
            has_absolute_patterns,
        })
    }

    fn is_match(&self, candidate: &Path) -> bool {
        if self.has_absolute_patterns
            && self.absolute_set.is_match(normalize_absolute_path_for_matcher(candidate))
        {
            return true;
        }

        if !self.has_relative_patterns {
            return false;
        }

        let relative_candidate = match &self.relative_root {
            Some(root) => {
                let Ok(relative) = candidate.strip_prefix(root) else {
                    return false;
                };
                normalize_relative_path_for_matcher(relative)
            }
            None => normalize_relative_path_for_matcher(candidate),
        };

        self.relative_set.is_match(relative_candidate)
    }

    fn matches_path_or_descendant(&self, candidate: &Path) -> bool {
        self.is_match(candidate)
            || self.is_match(&candidate.join("__tidysql_prune_probe__"))
            || self.is_match(&candidate.join("__tidysql_prune_probe__.sql"))
    }
}

impl ConfigResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_loaded_config_path(&self, path: &Path) -> bool {
        let path = normalize_path(path);
        self.state.read().unwrap().configs.contains_key(&path)
    }

    pub fn invalidate(&self) {
        let mut state = self.state.write().unwrap();
        state.nearest_configs.clear();
        state.configs.clear();
    }

    pub fn resolve_isolated(&self) -> Arc<ResolvedConfig> {
        self.default_config()
    }

    pub fn resolve(&self, source_path: &Path) -> Result<Arc<ResolvedConfig>, ConfigError> {
        let Some(config_path) = self.find_nearest_config_path(source_path) else {
            return Ok(self.default_config());
        };

        self.resolve_explicit(&config_path)
    }

    pub fn resolve_explicit(&self, path: &Path) -> Result<Arc<ResolvedConfig>, ConfigError> {
        self.resolve_config_path(path, &mut HashSet::new())
    }

    fn default_config(&self) -> Arc<ResolvedConfig> {
        self.default_config
            .get_or_init(|| {
                Arc::new(ResolvedConfig::default().expect("default config must compile"))
            })
            .clone()
    }

    fn resolve_config_path(
        &self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<Arc<ResolvedConfig>, ConfigError> {
        let path = normalize_path(path);
        if let Some(resolved) = self.state.read().unwrap().configs.get(&path).cloned() {
            return Ok(resolved);
        }

        if !visited.insert(path.clone()) {
            return Err(ConfigError::ExtendCycle { path });
        }

        let (path, input) = read_config(&path)?;
        let raw_config = parse_raw_config(&input, Some(path.clone()))?;
        let mut table = parse_config_table(&input, Some(path.clone()))?;
        let extend = resolve_extend_path(&path, raw_config.extend.as_deref())?;

        let parent = match extend {
            Some(extend_path) => self.resolve_config_path(&extend_path, visited)?,
            None => self.default_config(),
        };

        table.remove("extend");
        table.remove("files");

        let mut merged = config_to_table(parent.config().clone());
        merge_config_table(&mut merged, &mut table);
        let config = parse_config_from_table(merged, Some(path.clone()))?;
        let (files, include_matchers, exclude_matchers) =
            resolve_files_config(parent.as_ref(), &raw_config.files, &path)?;
        visited.remove(&path);
        let resolved = Arc::new(ResolvedConfig::new(
            config,
            files,
            Some(path.clone()),
            include_matchers,
            exclude_matchers,
        ));
        self.state.write().unwrap().configs.insert(path, resolved.clone());
        Ok(resolved)
    }

    fn find_nearest_config_path(&self, path: &Path) -> Option<PathBuf> {
        let start = normalize_start_dir(path);
        let mut traversed = Vec::new();
        let mut current = start.clone();

        loop {
            let cached = {
                let state = self.state.read().unwrap();
                state.nearest_configs.get(&current).cloned()
            };
            if let Some(cached) = cached {
                self.fill_nearest_cache(&traversed, cached.clone());
                return cached;
            }

            let candidate = current.join(DEFAULT_CONFIG_FILE);
            if candidate.is_file() {
                let candidate = normalize_path(&candidate);
                let hit = Some(candidate.clone());
                traversed.push(current.clone());
                self.fill_nearest_cache(&traversed, hit.clone());
                return hit;
            }

            traversed.push(current.clone());
            let Some(parent) = current.parent() else {
                self.fill_nearest_cache(&traversed, None);
                return None;
            };
            current = parent.to_path_buf();
        }
    }

    fn fill_nearest_cache(&self, traversed: &[PathBuf], value: Option<PathBuf>) {
        let mut state = self.state.write().unwrap();
        for dir in traversed {
            state.nearest_configs.insert(dir.clone(), value.clone());
        }
    }
}

fn default_base_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn normalize_start_dir(path: &Path) -> PathBuf {
    let absolute = normalize_path(path);
    if absolute.is_dir() {
        absolute
    } else {
        absolute.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    }
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let absolute =
        if path.is_absolute() { path.to_path_buf() } else { default_base_dir().join(path) };
    lexical_normalize_path(&absolute)
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut prefix = None;
    let mut has_root = false;
    let mut parts: Vec<std::ffi::OsString> = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            std::path::Component::RootDir => has_root = true,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if let Some(last) = parts.last()
                    && last != ".."
                {
                    parts.pop();
                    continue;
                }
                if !has_root {
                    parts.push("..".into());
                }
            }
            std::path::Component::Normal(part) => parts.push(part.to_os_string()),
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if has_root {
        normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR));
    }
    for part in parts {
        normalized.push(part);
    }

    if normalized.as_os_str().is_empty() { PathBuf::from(".") } else { normalized }
}

fn normalize_absolute_path_for_matcher(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_relative_path_for_matcher(path: &Path) -> String {
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => parts.push("..".to_string()),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
        }
    }

    if parts.is_empty() { ".".to_string() } else { parts.join("/") }
}

fn parse_config_table(input: &str, path: Option<PathBuf>) -> Result<toml::Table, ConfigError> {
    toml::from_str(input).map_err(|source| ConfigError::Toml { path, source: Box::new(source) })
}

fn parse_config_from_table(
    table: toml::Table,
    path: Option<PathBuf>,
) -> Result<Config, ConfigError> {
    toml::Value::Table(table)
        .try_into()
        .map_err(|source| ConfigError::Toml { path, source: Box::new(source) })
}

fn parse_raw_config(input: &str, path: Option<PathBuf>) -> Result<RawConfigFile, ConfigError> {
    toml::from_str(input).map_err(|source| ConfigError::Toml { path, source: Box::new(source) })
}

fn resolve_extend_path(
    config_path: &Path,
    extend_value: Option<&Path>,
) -> Result<Option<PathBuf>, ConfigError> {
    let Some(extend_value) = extend_value else {
        return Ok(None);
    };
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(Some(normalize_path(&parent.join(extend_value))))
}

fn merge_config_table(parent: &mut toml::Table, child: &mut toml::Table) {
    for (key, value) in std::mem::take(child) {
        match key.as_str() {
            "core" | "lints" => merge_nested_table(parent, key, value),
            _ => {
                parent.insert(key, value);
            }
        }
    }
}

fn merge_nested_table(parent: &mut toml::Table, key: String, value: toml::Value) {
    let toml::Value::Table(mut child_table) = value else {
        parent.insert(key, value);
        return;
    };

    if let Some(toml::Value::Table(parent_table)) = parent.get_mut(&key) {
        for (child_key, child_value) in std::mem::take(&mut child_table) {
            match (parent_table.get_mut(&child_key), child_value) {
                (Some(toml::Value::Table(parent_nested)), toml::Value::Table(mut child_nested)) => {
                    for (nested_key, nested_value) in std::mem::take(&mut child_nested) {
                        parent_nested.insert(nested_key, nested_value);
                    }
                }
                (_, child_value) => {
                    parent_table.insert(child_key, child_value);
                }
            }
        }
    } else {
        parent.insert(key, toml::Value::Table(child_table));
    }
}

fn config_to_table(config: Config) -> toml::Table {
    toml::Value::try_from(config)
        .expect("config should serialize")
        .as_table()
        .expect("config should serialize as a table")
        .clone()
}

fn resolve_files_config(
    parent: &ResolvedConfig,
    child: &RawFilesConfig,
    config_path: &Path,
) -> Result<(FilesConfig, Vec<CompiledGlobMatcher>, Vec<CompiledGlobMatcher>), ConfigError> {
    let mut files = parent.files.clone();
    files.extend_include.clear();
    files.extend_exclude.clear();

    let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let config_path = Some(config_path.to_path_buf());
    let mut include_matchers = parent.include_matchers.clone();
    let mut exclude_matchers = parent.exclude_matchers.clone();

    if let Some(include) = &child.include {
        files.include = include.clone();
        include_matchers =
            vec![CompiledGlobMatcher::compile(include, Some(base_dir), config_path.clone())?];
    }
    if let Some(extend_include) = &child.extend_include {
        files.include.extend(extend_include.clone());
        include_matchers.push(CompiledGlobMatcher::compile(
            extend_include,
            Some(base_dir),
            config_path.clone(),
        )?);
    }
    if let Some(exclude) = &child.exclude {
        files.exclude = exclude.clone();
        exclude_matchers =
            vec![CompiledGlobMatcher::compile(exclude, Some(base_dir), config_path.clone())?];
    }
    if let Some(extend_exclude) = &child.extend_exclude {
        files.exclude.extend(extend_exclude.clone());
        exclude_matchers.push(CompiledGlobMatcher::compile(
            extend_exclude,
            Some(base_dir),
            config_path.clone(),
        )?);
    }
    if let Some(respect_gitignore) = child.respect_gitignore {
        files.respect_gitignore = respect_gitignore;
    }
    if let Some(force_exclude) = child.force_exclude {
        files.force_exclude = force_exclude;
    }

    Ok((files, include_matchers, exclude_matchers))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

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

    #[test]
    fn string_config_rejects_extend_field() {
        let error = Config::from_toml_str(
            r#"
extend = "parent.toml"
"#,
        )
        .unwrap_err();

        let ConfigError::Toml { source, .. } = error else {
            panic!("expected toml error");
        };

        assert!(source.to_string().contains("unknown field `extend`"));
    }

    #[test]
    fn string_config_rejects_files_block() {
        let error = Config::from_toml_str(
            r#"
[files]
respect_gitignore = true
"#,
        )
        .unwrap_err();

        let ConfigError::Toml { source, .. } = error else {
            panic!("expected toml error");
        };

        assert!(source.to_string().contains("unknown field `files`"));
    }

    #[test]
    fn extended_configs_override_and_append_file_patterns() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("parent.toml");
        let child = dir.path().join("child.toml");

        fs::write(
            &parent,
            r#"
[files]
include = ["**/*.sql"]
exclude = ["vendor/**"]

[core]
dialect = "ansi"
"#,
        )
        .unwrap();

        fs::write(
            &child,
            r#"
extend = "parent.toml"

[files]
extend_include = ["**/*.ddl"]
extend_exclude = ["generated/**"]

[core]
dialect = "snowflake"
"#,
        )
        .unwrap();

        let resolved = ConfigResolver::new().resolve_explicit(&child).unwrap();
        assert_eq!(resolved.config().core.dialect, Dialect::Snowflake);
        assert!(resolved.matches_discovered_file(&dir.path().join("query.sql")));
        assert!(resolved.matches_discovered_file(&dir.path().join("schema.ddl")));
        assert!(!resolved.matches_discovered_file(&dir.path().join("generated/out.sql")));
        assert!(!resolved.matches_discovered_file(&dir.path().join("vendor/out.sql")));
    }

    #[test]
    fn resolver_rejects_unknown_files_keys() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("tidysql.toml");

        fs::write(
            &config_path,
            r#"
[files]
respect_gitigore = true
"#,
        )
        .unwrap();

        let error = ConfigResolver::new().resolve_explicit(&config_path).unwrap_err();

        let ConfigError::Toml { source, .. } = error else {
            panic!("expected toml error");
        };

        assert!(source.to_string().contains("unknown field `respect_gitigore`"));
    }

    #[test]
    fn resolver_picks_nearest_config() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("tidysql.toml");
        let nested_dir = dir.path().join("nested");
        let nested_config = nested_dir.join("tidysql.toml");
        let source_path = nested_dir.join("query.sql");

        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(
            &root,
            r#"
[core]
dialect = "ansi"
"#,
        )
        .unwrap();
        fs::write(
            &nested_config,
            r#"
[core]
dialect = "snowflake"
"#,
        )
        .unwrap();
        fs::write(&source_path, "select 1").unwrap();

        let resolved = ConfigResolver::new().resolve(&source_path).unwrap();
        assert_eq!(resolved.config_path(), Some(nested_config.as_path()));
        assert_eq!(resolved.config().core.dialect, Dialect::Snowflake);
    }

    #[test]
    fn extended_parent_patterns_keep_parent_base_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("tidysql.toml");
        let app_dir = dir.path().join("packages").join("app");
        let child = app_dir.join("tidysql.toml");
        let matching = app_dir.join("queries").join("a.sql");
        let excluded = app_dir.join("generated").join("b.sql");

        fs::create_dir_all(matching.parent().unwrap()).unwrap();
        fs::create_dir_all(excluded.parent().unwrap()).unwrap();
        fs::write(
            &root,
            r#"
[files]
include = ["packages/*/queries/*.sql"]
exclude = ["packages/*/generated/**"]
"#,
        )
        .unwrap();
        fs::write(&child, "extend = \"../../tidysql.toml\"\n").unwrap();
        fs::write(&matching, "select 1\n").unwrap();
        fs::write(&excluded, "select 1\n").unwrap();

        let resolved = ConfigResolver::new().resolve_explicit(&child).unwrap();
        assert!(resolved.matches_discovered_file(&matching));
        assert!(!resolved.matches_discovered_file(&excluded));
    }

    #[test]
    fn invalidate_clears_cached_resolution() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("tidysql.toml");
        let source_path = dir.path().join("query.sql");

        fs::write(
            &config_path,
            r#"
[core]
dialect = "ansi"
"#,
        )
        .unwrap();
        fs::write(&source_path, "select 1\n").unwrap();

        let resolver = ConfigResolver::new();
        let first = resolver.resolve(&source_path).unwrap();
        assert_eq!(first.config().core.dialect, Dialect::Ansi);

        fs::write(
            &config_path,
            r#"
[core]
dialect = "postgres"
"#,
        )
        .unwrap();

        let cached = resolver.resolve(&source_path).unwrap();
        assert_eq!(cached.config().core.dialect, Dialect::Ansi);

        resolver.invalidate();

        let refreshed = resolver.resolve(&source_path).unwrap();
        assert_eq!(refreshed.config().core.dialect, Dialect::Postgres);
    }
}
