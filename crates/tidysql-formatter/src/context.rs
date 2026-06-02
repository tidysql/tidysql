use tidysql_config::{Format, FormatCommaStyle, FormatKeywordCase};

use crate::FormatMode;

#[derive(Clone, Copy)]
pub(crate) struct SqlFormatContext<'a> {
    config: &'a Format,
    mode: FormatMode,
}

impl<'a> SqlFormatContext<'a> {
    pub(crate) fn new(config: &'a Format, mode: FormatMode) -> Self {
        Self { config, mode }
    }

    pub(crate) fn config(self) -> &'a Format {
        self.config
    }

    pub(crate) fn mode(self) -> FormatMode {
        self.mode
    }

    pub(crate) fn indent_width(self) -> usize {
        self.config.indent_width
    }

    pub(crate) fn comma_style(self) -> FormatCommaStyle {
        self.config.comma_style
    }

    pub(crate) fn keyword(self, keyword: &str) -> String {
        match self.config.keyword_case {
            FormatKeywordCase::Upper => keyword.to_ascii_uppercase(),
            FormatKeywordCase::Lower => keyword.to_ascii_lowercase(),
            FormatKeywordCase::Preserve => keyword.to_string(),
        }
    }
}
