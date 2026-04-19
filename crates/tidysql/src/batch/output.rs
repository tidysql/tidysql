use std::collections::BTreeMap;

use super::{BatchCommandKind, FileOutcome, FileResult};
use crate::diagnostics::{check_diagnostics, emit_diagnostics};

#[derive(Default)]
struct BatchSummary {
    had_processing_error: bool,
    had_check_diagnostics: bool,
    had_format_differences: bool,
}

pub(super) struct BatchAggregator {
    command: BatchCommandKind,
    next_seq: usize,
    pending: BTreeMap<usize, FileResult>,
    summary: BatchSummary,
}

impl BatchAggregator {
    pub(super) fn new(command: BatchCommandKind) -> Self {
        Self { command, next_seq: 0, pending: BTreeMap::new(), summary: BatchSummary::default() }
    }

    pub(super) fn push(&mut self, result: FileResult) {
        self.pending.insert(result.seq, result);
        for ready in take_ready_results(&mut self.pending, &mut self.next_seq) {
            handle_result(&ready, self.command, &mut self.summary);
        }
    }

    pub(super) fn finish(self) -> Result<(), String> {
        match self.command {
            BatchCommandKind::Check { .. } => {
                if self.summary.had_processing_error {
                    Err("lint check failed: at least one file could not be processed".to_string())
                } else if self.summary.had_check_diagnostics {
                    Err("lint check failed: diagnostics with error or warning severity found"
                        .to_string())
                } else {
                    Ok(())
                }
            }
            BatchCommandKind::FormatWrite => {
                if self.summary.had_processing_error {
                    Err("format failed: at least one file could not be processed".to_string())
                } else {
                    Ok(())
                }
            }
            BatchCommandKind::FormatCheck => {
                if self.summary.had_processing_error {
                    Err("format check failed: at least one file could not be processed".to_string())
                } else if self.summary.had_format_differences {
                    Err("format check failed: files require formatting".to_string())
                } else {
                    Ok(())
                }
            }
        }
    }
}

fn take_ready_results(
    pending: &mut BTreeMap<usize, FileResult>,
    next_seq: &mut usize,
) -> Vec<FileResult> {
    let mut ready = Vec::new();
    while let Some(result) = pending.remove(next_seq) {
        ready.push(result);
        *next_seq += 1;
    }
    ready
}

fn handle_result(result: &FileResult, command: BatchCommandKind, summary: &mut BatchSummary) {
    match &result.outcome {
        FileOutcome::Check { source, diagnostics } => {
            emit_diagnostics(&result.display_path, source, diagnostics);
            if check_diagnostics(diagnostics).is_err() {
                summary.had_check_diagnostics = true;
            }
        }
        FileOutcome::FormatCheck { needs_rewrite } => {
            if *needs_rewrite {
                eprintln!("{}", result.display_path);
                summary.had_format_differences = true;
            }
        }
        FileOutcome::Success => {}
        FileOutcome::Error(message) => {
            summary.had_processing_error = true;
            match command {
                BatchCommandKind::Check { .. }
                | BatchCommandKind::FormatWrite
                | BatchCommandKind::FormatCheck => {
                    eprintln!("{}: {message}", result.display_path);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{FileOutcome, FileResult, take_ready_results};

    #[test]
    fn take_ready_results_waits_for_the_next_sequence() {
        let mut pending = BTreeMap::new();
        let mut next_seq = 0usize;
        pending.insert(
            1,
            FileResult { seq: 1, display_path: "b.sql".to_string(), outcome: FileOutcome::Success },
        );

        assert!(take_ready_results(&mut pending, &mut next_seq).is_empty());

        pending.insert(
            0,
            FileResult { seq: 0, display_path: "a.sql".to_string(), outcome: FileOutcome::Success },
        );

        let ready = take_ready_results(&mut pending, &mut next_seq);
        assert_eq!(ready.into_iter().map(|result| result.seq).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(next_seq, 2);
    }
}
