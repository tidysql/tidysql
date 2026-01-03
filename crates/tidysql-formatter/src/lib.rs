use tidysql_syntax::DialectKind;

pub fn format_with_dialect(source: &str, _dialect: DialectKind) -> String {
    source.to_string()
}
