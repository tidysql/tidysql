use std::path::PathBuf;
use std::sync::Arc;

pub(crate) mod discovery;
pub(crate) mod executor;
pub(crate) mod output;

pub(crate) use discovery::execution_plan;
pub(crate) use executor::run_batch;

#[derive(Clone)]
pub(crate) struct BatchInputPlan {
    pub(crate) explicit_files: Vec<PathBuf>,
    pub(crate) directory_roots: Vec<PathBuf>,
    pub(crate) glob_patterns: Vec<String>,
}

pub(crate) enum ExecutionPlan {
    Stdin,
    SingleFile(PathBuf),
    Batch(BatchInputPlan),
}

#[derive(Clone)]
pub(crate) struct FileJob {
    pub(crate) seq: usize,
    pub(crate) path: PathBuf,
    pub(crate) display_path: String,
    pub(crate) resolved_config: Arc<tidysql_config::ResolvedConfig>,
}

pub(crate) struct FileResult {
    pub(crate) seq: usize,
    pub(crate) display_path: String,
    pub(crate) outcome: FileOutcome,
}

pub(crate) enum FileOutcome {
    Check { source: String, diagnostics: Vec<tidysql::Diagnostic> },
    FormatCheck { original: String, formatted: String },
    Success,
    Error(String),
}

#[derive(Clone, Copy)]
pub(crate) enum BatchCommandKind {
    Check { fix: bool },
    FormatWrite { strict: bool },
    FormatCheck { strict: bool },
}
