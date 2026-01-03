use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Core {
    pub dialect: Dialect,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub core: Core,
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
