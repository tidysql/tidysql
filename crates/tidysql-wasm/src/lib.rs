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
    severity: &'static str,
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
#[derive(Default)]
pub struct Workspace;

#[wasm_bindgen]
impl Workspace {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Workspace {
        Self
    }

    pub fn check_with_config(&self, source: &str, config_toml: &str) -> Result<JsValue, JsValue> {
        let config = parse_config(config_toml)?;

        let diagnostics =
            to_monaco_diagnostics(source, tidysql::check_with_config(source, &config), "sql");

        to_js_value(&diagnostics)
    }

    pub fn dialects(&self) -> Result<JsValue, JsValue> {
        let dialects = tidysql_config::DIALECTS
            .iter()
            .map(|dialect| DialectInfo { id: dialect.as_str(), label: dialect.label() })
            .collect::<Vec<_>>();

        to_js_value(&dialects)
    }

    pub fn format_with_config(&self, source: &str, config_toml: &str) -> Result<String, JsValue> {
        let config = parse_config(config_toml)?;

        let formatted = tidysql::format_with_config(source, &config)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(formatted)
    }

    pub fn fix_with_config(&self, source: &str, config_toml: &str) -> Result<String, JsValue> {
        let config = parse_config(config_toml)?;

        tidysql::fix_with_config(source, &config)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

fn parse_config(config_toml: &str) -> Result<tidysql_config::Config, JsValue> {
    tidysql_config::Config::from_toml_str(config_toml)
        .map_err(|error| config_error_value(config_toml, &error))
}

fn config_error_value(config_toml: &str, error: &tidysql_config::ConfigError) -> JsValue {
    let range = config_error_range(error);
    let diagnostics = [MonacoDiagnostic {
        message: config_error_message(error),
        severity: map_severity(tidysql::Severity::Error),
        start: utf16_position(config_toml, range.start),
        end: utf16_position(config_toml, range.end),
        source: "config",
    }];

    to_js_value(&diagnostics).unwrap_or_else(|error| error)
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn map_severity(severity: tidysql::Severity) -> &'static str {
    match severity {
        tidysql::Severity::Error => "error",
        tidysql::Severity::Warn => "warning",
        tidysql::Severity::Info => "info",
        tidysql::Severity::Hint => "hint",
        tidysql::Severity::Allow => unreachable!("Allow diagnostics should be suppressed earlier"),
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
        } else if ch != '\r' {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_position_crlf() {
        // "AB\r\nCD" — position of 'C' (byte index 4)
        let source = "AB\r\nCD";
        let pos = utf16_position(source, 4);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn utf16_position_crlf_mid_line() {
        // "AB\r\nCD" — position of 'D' (byte index 5)
        let source = "AB\r\nCD";
        let pos = utf16_position(source, 5);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 2);
    }

    #[test]
    fn utf16_position_cr_does_not_add_column() {
        // At position right after 'B' and before '\r' (byte index 2)
        let source = "AB\r\nCD";
        let pos = utf16_position(source, 2);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 3);
    }
}
