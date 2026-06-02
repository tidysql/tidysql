#![allow(dead_code)]

use tidysql_config::Format;
use tidysql_syntax::DialectKind;

mod builders;
mod comments;
mod context;
mod doc;
mod error;
mod printer;
mod tokens;

#[cfg(test)]
mod tests;

use doc::render;
pub use error::FormatError;
use printer::SqlPrinter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    Pragmatic,
    Strict,
}

pub fn format_with_dialect(source: &str, dialect: DialectKind) -> Result<String, FormatError> {
    format_with_config(source, dialect, &Format::default())
}

pub fn format_with_config(
    source: &str,
    dialect: DialectKind,
    config: &Format,
) -> Result<String, FormatError> {
    format_with_config_and_mode(source, dialect, config, FormatMode::Pragmatic)
}

pub fn format_with_config_strict(
    source: &str,
    dialect: DialectKind,
    config: &Format,
) -> Result<String, FormatError> {
    format_with_config_and_mode(source, dialect, config, FormatMode::Strict)
}

pub fn format_with_config_and_mode(
    source: &str,
    dialect: DialectKind,
    config: &Format,
    mode: FormatMode,
) -> Result<String, FormatError> {
    let tree = tidysql_syntax::parse(source, dialect)?;
    let doc = SqlPrinter::new(config, mode).format_root(&tree.root())?;
    let mut output = render(doc, config.line_width.max(1));

    if source.ends_with('\n') {
        if !output.ends_with('\n') {
            output.push('\n');
        }
    } else {
        while output.ends_with('\n') {
            output.pop();
        }
    }

    Ok(output)
}
