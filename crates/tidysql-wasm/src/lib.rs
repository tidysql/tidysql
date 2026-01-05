use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct MonacoPosition {
    line: u32,
    column: u32,
}

#[derive(Serialize)]
struct MonacoDiagnostic {
    message: String,
    severity: String,
    start: MonacoPosition,
    end: MonacoPosition,
    source: &'static str,
}

#[derive(Serialize)]
struct DialectInfo {
    id: &'static str,
    label: &'static str,
}

#[wasm_bindgen]
pub struct Workspace;

#[wasm_bindgen]
impl Workspace {
    #[allow(clippy::new_without_default)]
    #[wasm_bindgen(constructor)]
    pub fn new() -> Workspace {
        Workspace
    }

    pub fn check_with_config(&self, source: &str, config_toml: &str) -> Result<JsValue, JsValue> {
        let config = match tidysql_config::Config::from_toml_str(config_toml) {
            Ok(config) => config,
            Err(error) => {
                let range = config_error_range(&error);
                let message = config_error_message(&error);
                let diagnostics = vec![MonacoDiagnostic {
                    message,
                    severity: map_severity(tidysql::Severity::Error),
                    start: utf16_position(config_toml, range.start),
                    end: utf16_position(config_toml, range.end),
                    source: "config",
                }];

                return serde_wasm_bindgen::to_value(&diagnostics)
                    .map_err(|error| JsValue::from_str(&error.to_string()));
            }
        };

        let diagnostics =
            to_monaco_diagnostics(source, tidysql::check_with_config(source, &config), "sql");

        serde_wasm_bindgen::to_value(&diagnostics)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn dialects(&self) -> Result<JsValue, JsValue> {
        let dialects = tidysql_config::DIALECTS
            .iter()
            .map(|dialect| DialectInfo { id: dialect.as_str(), label: dialect.label() })
            .collect::<Vec<_>>();

        serde_wasm_bindgen::to_value(&dialects)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn format_with_config(&self, source: &str, config_toml: &str) -> Result<String, JsValue> {
        let config = match tidysql_config::Config::from_toml_str(config_toml) {
            Ok(config) => config,
            Err(error) => {
                let range = config_error_range(&error);
                let message = config_error_message(&error);
                let diagnostics = vec![MonacoDiagnostic {
                    message,
                    severity: map_severity(tidysql::Severity::Error),
                    start: utf16_position(config_toml, range.start),
                    end: utf16_position(config_toml, range.end),
                    source: "config",
                }];

                let value = serde_wasm_bindgen::to_value(&diagnostics)
                    .map_err(|error| JsValue::from_str(&error.to_string()))?;
                return Err(value);
            }
        };

        let formatted = tidysql::format_with_config(source, &config);
        Ok(formatted)
    }
}

fn map_severity(severity: tidysql::Severity) -> String {
    match severity {
        tidysql::Severity::Error => "error".to_string(),
        tidysql::Severity::Warning => "warning".to_string(),
        tidysql::Severity::Info => "info".to_string(),
        tidysql::Severity::Hint => "hint".to_string(),
    }
}

fn to_monaco_diagnostics(
    source: &str,
    diagnostics: Vec<tidysql::Diagnostic>,
    diagnostic_source: &'static str,
) -> Vec<MonacoDiagnostic> {
    diagnostics
        .into_iter()
        .map(|diagnostic| MonacoDiagnostic {
            message: diagnostic.message,
            severity: map_severity(diagnostic.severity),
            start: utf16_position(source, diagnostic.range.start),
            end: utf16_position(source, diagnostic.range.end),
            source: diagnostic_source,
        })
        .collect()
}

fn utf16_position(source: &str, byte_index: usize) -> MonacoPosition {
    let target = byte_index.min(source.len());
    let mut line = 1u32;
    let mut column = 1u32;
    let mut offset = 0usize;

    for ch in source.chars() {
        if offset >= target {
            break;
        }

        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += ch.len_utf16() as u32;
        }

        offset += ch.len_utf8();
    }

    MonacoPosition { line, column }
}

fn config_error_range(error: &tidysql_config::ConfigError) -> std::ops::Range<usize> {
    match error {
        tidysql_config::ConfigError::Toml { source, .. } => source.span().unwrap_or(0..0),
        _ => 0..0,
    }
}

fn config_error_message(error: &tidysql_config::ConfigError) -> String {
    match error {
        tidysql_config::ConfigError::Toml { source, .. } => {
            format!("Config error: {}", source.message())
        }
        _ => format!("Config error: {error}"),
    }
}
