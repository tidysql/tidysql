use std::path::Path;
use std::thread;

use crossbeam_channel::bounded;

use super::discovery::discover_jobs_with;
use super::output::BatchAggregator;
use super::{BatchCommandKind, BatchInputPlan, FileJob, FileOutcome, FileResult};
use crate::{ConfigArguments, atomic_write};

pub(crate) fn run_batch(
    plan: BatchInputPlan,
    command: BatchCommandKind,
    jobs: Option<usize>,
    config_arguments: &ConfigArguments,
) -> Result<(), String> {
    let worker_count = worker_count(jobs);
    let capacity = batch_channel_capacity(worker_count);
    let (job_tx, job_rx) = bounded(capacity);
    let (result_tx, result_rx) = bounded(capacity);

    let producer_config = config_arguments.clone();
    let producer_results = result_tx.clone();
    let producer = thread::spawn(move || {
        discover_jobs_with(
            plan,
            &producer_config,
            |job| job_tx.send(job).is_ok(),
            |result| producer_results.send(result).is_ok(),
        )
    });

    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let worker_config = config_arguments.clone();
        let worker_jobs = job_rx.clone();
        let worker_tx = result_tx.clone();
        workers.push(thread::spawn(move || {
            while let Ok(job) = worker_jobs.recv() {
                if worker_tx.send(process_job(job, command, &worker_config)).is_err() {
                    break;
                }
            }
        }));
    }
    drop(job_rx);
    drop(result_tx);

    let mut aggregator = BatchAggregator::new(command);
    for result in result_rx {
        aggregator.push(result);
    }

    producer.join().map_err(|_| "batch discovery thread panicked".to_string())??;
    for worker in workers {
        worker.join().map_err(|_| "batch worker thread panicked".to_string())?;
    }

    aggregator.finish()
}

fn worker_count(requested: Option<usize>) -> usize {
    requested.filter(|jobs| *jobs > 0).unwrap_or_else(|| {
        thread::available_parallelism().map(|parallelism| parallelism.get()).unwrap_or(1)
    })
}

fn batch_channel_capacity(worker_count: usize) -> usize {
    worker_count.max(1) * 4
}

fn process_job(
    job: FileJob,
    command: BatchCommandKind,
    config_arguments: &ConfigArguments,
) -> FileResult {
    let input = match std::fs::read_to_string(&job.path) {
        Ok(input) => input,
        Err(error) => {
            return FileResult {
                seq: job.seq,
                display_path: job.display_path,
                outcome: FileOutcome::Error(error.to_string()),
            };
        }
    };
    let config = config_arguments.apply_overrides(job.resolved_config.config());

    let outcome = match command {
        BatchCommandKind::Check { fix } => process_check_file(&job.path, input, &config, fix),
        BatchCommandKind::FormatWrite { strict } => {
            process_format_write(&job.path, input, &config, strict)
        }
        BatchCommandKind::FormatCheck { strict } => process_format_check(input, &config, strict),
    };
    FileResult { seq: job.seq, display_path: job.display_path, outcome }
}

fn process_check_file(
    path: &Path,
    input: String,
    config: &tidysql_config::Config,
    fix: bool,
) -> FileOutcome {
    let checked_source = if fix {
        match tidysql::fix_with_config(&input, config) {
            Ok(fixed) => {
                if fixed != input
                    && let Err(error) = atomic_write(path, &fixed)
                {
                    return FileOutcome::Error(error.to_string());
                }
                fixed
            }
            Err(error) => return FileOutcome::Error(error.to_string()),
        }
    } else {
        input
    };

    let diagnostics = tidysql::check_with_config(&checked_source, config);
    FileOutcome::Check { source: checked_source, diagnostics }
}

fn process_format_write(
    path: &Path,
    input: String,
    config: &tidysql_config::Config,
    strict: bool,
) -> FileOutcome {
    let formatted = match format_source(&input, config, strict) {
        Ok(formatted) => formatted,
        Err(error) => return FileOutcome::Error(error.to_string()),
    };

    if formatted != input
        && let Err(error) = atomic_write(path, &formatted)
    {
        return FileOutcome::Error(error.to_string());
    }

    FileOutcome::Success
}

fn process_format_check(
    input: String,
    config: &tidysql_config::Config,
    strict: bool,
) -> FileOutcome {
    match format_source(&input, config, strict) {
        Ok(formatted) => FileOutcome::FormatCheck { needs_rewrite: formatted != input },
        Err(error) => FileOutcome::Error(error.to_string()),
    }
}

fn format_source(
    input: &str,
    config: &tidysql_config::Config,
    strict: bool,
) -> Result<String, tidysql_formatter::FormatError> {
    if strict {
        tidysql::format_with_config_strict(input, config)
    } else {
        tidysql::format_with_config(input, config)
    }
}
