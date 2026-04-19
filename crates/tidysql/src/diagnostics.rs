use std::io::{self, IsTerminal};
use std::ops::Range;

use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};

pub(crate) fn emit_diagnostics(path: &str, source: &str, diagnostics: &[tidysql::Diagnostic]) {
    let renderer = if io::stderr().is_terminal() { Renderer::styled() } else { Renderer::plain() };

    for diagnostic in diagnostics {
        let level = level_for_severity(diagnostic.severity);
        let range = clamp_range(diagnostic.range.clone(), source.len());
        let snippet = Snippet::source(source)
            .line_start(1)
            .path(path)
            .annotation(AnnotationKind::Primary.span(range).label(diagnostic.message.as_str()));
        let mut group =
            level.primary_title(diagnostic.message.as_str()).id(diagnostic.code).element(snippet);

        if let Some(fix) = &diagnostic.fix {
            group = group.element(Level::HELP.message(format!("fix: {}", fix.title)));
        }

        let report = [group];
        eprintln!("{}", renderer.render(&report));
    }
}

pub(crate) fn check_diagnostics(diagnostics: &[tidysql::Diagnostic]) -> Result<(), String> {
    let has_failing = diagnostics.iter().any(|diagnostic| {
        matches!(diagnostic.severity, tidysql::Severity::Error | tidysql::Severity::Warn)
    });

    if has_failing {
        Err("lint check failed: diagnostics with error or warning severity found".to_string())
    } else {
        Ok(())
    }
}

fn level_for_severity(severity: tidysql::Severity) -> Level<'static> {
    match severity {
        tidysql::Severity::Error => Level::ERROR,
        tidysql::Severity::Warn => Level::WARNING,
        tidysql::Severity::Info => Level::INFO,
        tidysql::Severity::Hint => Level::HELP,
        tidysql::Severity::Allow => unreachable!("Allow diagnostics should be suppressed earlier"),
    }
}

fn clamp_range(range: Range<usize>, source_len: usize) -> Range<usize> {
    let max = source_len.saturating_add(1);
    let start = range.start.min(max);
    let end = range.end.min(max);

    if end < start { start..start } else { start..end }
}
