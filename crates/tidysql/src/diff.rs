use similar::TextDiff;

pub(crate) fn emit_format_diff(display_path: &str, original: &str, formatted: &str) {
    eprint!("{}", format_diff(display_path, original, formatted));
}

fn format_diff(display_path: &str, original: &str, formatted: &str) -> String {
    TextDiff::from_lines(original, formatted)
        .unified_diff()
        .header(&format!("{} (original)", display_path), &format!("{} (formatted)", display_path))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::format_diff;

    #[test]
    fn format_diff_uses_unified_headers_and_hunks() {
        let diff = format_diff("query.sql", "select a,b from foo", "SELECT a, b\nFROM foo");

        assert!(diff.contains("--- query.sql (original)"));
        assert!(diff.contains("+++ query.sql (formatted)"));
        assert!(diff.contains("-select a,b from foo"));
        assert!(diff.contains("+SELECT a, b"));
        assert!(diff.contains("+FROM foo"));
    }
}
