use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tidysql", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Format {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
        #[arg(short, long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
    Check {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
        #[arg(short, long, value_name = "PATH")]
        config: Option<PathBuf>,
        #[arg(long)]
        fix: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Format { path, config } => format_cmd(path, config),
        Command::Check { path, config, fix } => check_cmd(path, config, fix),
    };

    if let Err(message) = result {
        if !message.is_empty() {
            eprintln!("{message}");
        }
        process::exit(1);
    }
}

fn format_cmd(path: Option<PathBuf>, config: Option<PathBuf>) -> Result<(), String> {
    let input = read_input(path.as_deref()).map_err(|err| err.to_string())?;
    let source_path = path.as_deref().unwrap_or_else(|| Path::new("."));
    let config = tidysql_config::load_config(config.as_deref(), source_path)
        .map_err(|err| err.to_string())?;

    let formatted = tidysql::format_with_config(&input, &config);
    write_output(&formatted).map_err(|err| err.to_string())
}

fn check_cmd(path: Option<PathBuf>, config: Option<PathBuf>, fix: bool) -> Result<(), String> {
    let input = read_input(path.as_deref()).map_err(|err| err.to_string())?;
    let source_path = path.as_deref().unwrap_or_else(|| Path::new("."));
    let config = tidysql_config::load_config(config.as_deref(), source_path)
        .map_err(|err| err.to_string())?;
    let display_path = path
        .as_deref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<stdin>".to_string());

    if fix {
        let fixed = tidysql::fix_with_config(&input, &config).map_err(|err| err.to_string())?;
        write_output(&fixed).map_err(|err| err.to_string())?;
        let diagnostics = tidysql::check_with_config(&fixed, &config);
        emit_diagnostics(&display_path, &fixed, &diagnostics);
        return check_diagnostics(&diagnostics);
    }

    let diagnostics = tidysql::check_with_config(&input, &config);
    emit_diagnostics(&display_path, &input, &diagnostics);
    check_diagnostics(&diagnostics)
}

fn read_input(path: Option<&Path>) -> io::Result<String> {
    match path {
        Some(path) => std::fs::read_to_string(path),
        None => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input)?;
            Ok(input)
        }
    }
}

fn write_output(output: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout.write_all(output.as_bytes())?;
    Ok(())
}

fn emit_diagnostics(path: &str, source: &str, diagnostics: &[tidysql::Diagnostic]) {
    for diagnostic in diagnostics {
        let (line, column) = line_column(source, diagnostic.range.start);
        let severity = severity_label(diagnostic.severity);
        eprintln!("{path}:{line}:{column}: {severity} {}: {}", diagnostic.code, diagnostic.message);
    }
}

fn check_diagnostics(diagnostics: &[tidysql::Diagnostic]) -> Result<(), String> {
    let has_failing = diagnostics.iter().any(|diagnostic| {
        matches!(diagnostic.severity, tidysql::Severity::Error | tidysql::Severity::Warn)
    });

    if has_failing { Err(String::new()) } else { Ok(()) }
}

fn severity_label(severity: tidysql::Severity) -> &'static str {
    match severity {
        tidysql::Severity::Error => "error",
        tidysql::Severity::Warn => "warning",
        tidysql::Severity::Info => "info",
        tidysql::Severity::Hint => "hint",
    }
}

fn line_column(source: &str, byte_index: usize) -> (usize, usize) {
    let target = byte_index.min(source.len());
    let mut line = 1usize;
    let mut column = 1usize;
    let mut offset = 0usize;

    for ch in source.chars() {
        if offset >= target {
            break;
        }

        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }

        offset += ch.len_utf8();
    }

    (line, column)
}
