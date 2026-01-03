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
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Format { path, config } => format_cmd(path, config),
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
