use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::de::{Deserializer, Error as DeError, IntoDeserializer, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use serde_untagged::UntaggedEnumVisitor;

pub const DEFAULT_CONFIG_FILE: &str = "tidysql.toml";

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
        let mut expected = String::new();
        for (index, dialect) in DIALECTS.iter().enumerate() {
            if index > 0 {
                expected.push_str(", ");
            }
            expected.push_str(dialect.as_str());
        }

        write!(f, "invalid dialect '{}', expected one of: {expected}", self.input)
    }
}

impl std::error::Error for DialectParseError {}

impl std::str::FromStr for Dialect {
    type Err = DialectParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.to_ascii_lowercase();
        DIALECTS
            .iter()
            .copied()
            .find(|dialect| normalized == dialect.as_str())
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
    DisallowNames,
    ExplicitUnion,
    InconsistentCapitalisation,
}

impl LintName {
    pub const fn as_str(&self) -> &'static str {
        match self {
            LintName::DisallowNames => "disallow_names",
            LintName::ExplicitUnion => "explicit_union",
            LintName::InconsistentCapitalisation => "inconsistent_capitalisation",
        }
    }
}

pub const LINTS: &[LintName] =
    &[LintName::DisallowNames, LintName::ExplicitUnion, LintName::InconsistentCapitalisation];

#[derive(Debug, Clone)]
pub struct LintNameParseError {
    input: String,
}

impl fmt::Display for LintNameParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut expected = String::new();
        for (index, lint) in LINTS.iter().enumerate() {
            if index > 0 {
                expected.push_str(", ");
            }
            expected.push_str(lint.as_str());
        }

        write!(f, "invalid lint '{}', expected one of: {expected}", self.input)
    }
}

impl std::error::Error for LintNameParseError {}

impl std::str::FromStr for LintName {
    type Err = LintNameParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.to_ascii_lowercase().replace('-', "_");
        LINTS
            .iter()
            .copied()
            .find(|lint| normalized == lint.as_str())
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct InconsistentCapitalisationConfig {
    pub capitalisation_policy: CapitalisationPolicy,
    pub ignore_words: Vec<String>,
    #[serde(
        deserialize_with = "deserialize_ignore_words_regex",
        serialize_with = "serialize_ignore_words_regex",
        default
    )]
    pub ignore_words_regex: Vec<Regex>,
}

fn deserialize_ignore_words_regex<'de, D>(deserializer: D) -> Result<Vec<Regex>, D::Error>
where
    D: Deserializer<'de>,
{
    struct RegexesVisitor;

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
                            "invalid lints.inconsistent_capitalisation.\
                             ignore_words_regex[{index}] (`{pattern}`): {error}"
                        )));
                    }
                }
                index += 1;
            }

            Ok(compiled)
        }
    }

    deserializer.deserialize_seq(RegexesVisitor)
}

fn serialize_ignore_words_regex<S>(regexes: &[Regex], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq = serializer.serialize_seq(Some(regexes.len()))?;
    for regex in regexes {
        seq.serialize_element(regex.as_str())?;
    }
    seq.end()
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DisallowNamesConfig {
    pub names: Vec<String>,
    #[serde(serialize_with = "serialize_disallow_name_regexes")]
    pub regexes: Vec<Regex>,
}

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
    struct RegexesVisitor;

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
                            "invalid lints.disallow_names.regexes[{index}] (`{pattern}`): {error}"
                        )));
                    }
                }
                index += 1;
            }

            Ok(compiled)
        }
    }

    deserializer.deserialize_seq(RegexesVisitor)
}

fn serialize_disallow_name_regexes<S>(regexes: &[Regex], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq = serializer.serialize_seq(Some(regexes.len()))?;
    for regex in regexes {
        seq.serialize_element(regex.as_str())?;
    }
    seq.end()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lints {
    pub disallow_names: LintConfig<DisallowNamesConfig>,
    pub explicit_union: LintConfig<ExplicitUnionConfig>,
    pub inconsistent_capitalisation: LintConfig<InconsistentCapitalisationConfig>,
}

impl Default for Lints {
    fn default() -> Self {
        Self {
            disallow_names: LintConfig::default(),
            explicit_union: LintConfig::default(),
            inconsistent_capitalisation: LintConfig {
                level: Severity::Allow,
                options: InconsistentCapitalisationConfig::default(),
            },
        }
    }
}

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
