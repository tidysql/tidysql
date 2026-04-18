use std::io::{self, IsTerminal, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process;

use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use clap::{Args, Parser, Subcommand};

mod lsp;

#[derive(Parser)]
#[command(name = "tidysql", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    global_options: GlobalConfigArgs,
}

#[derive(Args)]
struct GlobalConfigArgs {
    #[arg(short, long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
}

#[derive(Args)]
struct ConfigOverrideArgs {
    #[arg(long, value_name = "DIALECT")]
    dialect: Option<tidysql_config::Dialect>,
    #[arg(short = 'A', long, value_name = "LINT")]
    allow: Vec<tidysql_config::LintName>,
    #[arg(short = 'W', long, value_name = "LINT")]
    warn: Vec<tidysql_config::LintName>,
    #[arg(short = 'D', long, value_name = "LINT")]
    deny: Vec<tidysql_config::LintName>,
}

#[derive(Subcommand)]
enum Command {
    Format(FormatCommand),
    Check(CheckCommand),
    Lsp(LspCommand),
}

#[derive(Args)]
struct FormatCommand {
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    #[command(flatten)]
    config_overrides: ConfigOverrideArgs,
}

#[derive(Args)]
struct CheckCommand {
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    #[command(flatten)]
    config_overrides: ConfigOverrideArgs,
    #[arg(long)]
    fix: bool,
}

#[derive(Args)]
struct LspCommand {
    #[command(flatten)]
    config_overrides: ConfigOverrideArgs,
}

struct LoadedSource {
    input: String,
    config: tidysql_config::Config,
    display_path: String,
}

struct ConfigArguments {
    config_path: Option<PathBuf>,
    overrides: ConfigOverrides,
}

#[derive(Default)]
struct ConfigOverrides {
    dialect: Option<tidysql_config::Dialect>,
    lint_levels: Vec<LintLevelOverride>,
}

struct LintLevelOverride {
    lint: tidysql_config::LintName,
    level: tidysql_config::Severity,
}

impl ConfigOverrides {
    fn apply(&self, config: &mut tidysql_config::Config) {
        if let Some(dialect) = self.dialect {
            config.core.dialect = dialect;
        }

        for lint_override in &self.lint_levels {
            config.set_lint_level(lint_override.lint, lint_override.level);
        }
    }

    fn extend_lint_levels(
        lint_levels: &mut Vec<LintLevelOverride>,
        lints: Vec<tidysql_config::LintName>,
        level: tidysql_config::Severity,
    ) {
        lint_levels.extend(lints.into_iter().map(|lint| LintLevelOverride { lint, level }));
    }
}

impl From<ConfigOverrideArgs> for ConfigOverrides {
    fn from(args: ConfigOverrideArgs) -> Self {
        let mut lint_levels = Vec::new();
        Self::extend_lint_levels(&mut lint_levels, args.allow, tidysql_config::Severity::Allow);
        Self::extend_lint_levels(&mut lint_levels, args.warn, tidysql_config::Severity::Warn);
        Self::extend_lint_levels(&mut lint_levels, args.deny, tidysql_config::Severity::Error);

        Self { dialect: args.dialect, lint_levels }
    }
}

impl ConfigArguments {
    fn from_cli_arguments(global_options: GlobalConfigArgs, overrides: ConfigOverrides) -> Self {
        Self { config_path: global_options.config, overrides }
    }

    fn load_config(&self, source_path: &Path) -> Result<tidysql_config::Config, String> {
        let mut config = tidysql_config::load_config(self.config_path.as_deref(), source_path)
            .map_err(|err| err.to_string())?;
        self.overrides.apply(&mut config);
        Ok(config)
    }
}

fn config_arguments(
    global_options: GlobalConfigArgs,
    config_overrides: ConfigOverrideArgs,
) -> ConfigArguments {
    ConfigArguments::from_cli_arguments(global_options, config_overrides.into())
}

fn main() {
    let result = match Cli::parse() {
        Cli { command: Command::Format(args), global_options } => format(args, global_options),
        Cli { command: Command::Check(args), global_options } => check(args, global_options),
        Cli { command: Command::Lsp(args), global_options } => serve_lsp(args, global_options),
    };

    if let Err(message) = result {
        if !message.is_empty() {
            eprintln!("{message}");
        }
        process::exit(1);
    }
}

fn format(args: FormatCommand, global_options: GlobalConfigArgs) -> Result<(), String> {
    let FormatCommand { path, config_overrides } = args;
    let config_arguments = config_arguments(global_options, config_overrides);
    let LoadedSource { input, config, .. } = load_source(path.as_deref(), &config_arguments)?;

    let formatted = tidysql::format_with_config(&input, &config).map_err(|err| err.to_string())?;
    write_output(&formatted).map_err(|err| err.to_string())
}

fn check(args: CheckCommand, global_options: GlobalConfigArgs) -> Result<(), String> {
    let CheckCommand { path, config_overrides, fix } = args;
    let config_arguments = config_arguments(global_options, config_overrides);
    let LoadedSource { input, config, display_path } =
        load_source(path.as_deref(), &config_arguments)?;

    let checked_source = if fix {
        let fixed = tidysql::fix_with_config(&input, &config).map_err(|err| err.to_string())?;
        match path.as_deref() {
            Some(path) => atomic_write(path, &fixed).map_err(|err| err.to_string())?,
            None => write_output(&fixed).map_err(|err| err.to_string())?,
        }
        fixed
    } else {
        input
    };

    let diagnostics = tidysql::check_with_config(&checked_source, &config);
    emit_diagnostics(&display_path, &checked_source, &diagnostics);
    check_diagnostics(&diagnostics)
}

fn serve_lsp(args: LspCommand, global_options: GlobalConfigArgs) -> Result<(), String> {
    lsp::run(config_arguments(global_options, args.config_overrides))
}

fn load_source(
    path: Option<&Path>,
    config_arguments: &ConfigArguments,
) -> Result<LoadedSource, String> {
    let input = read_input(path).map_err(|err| err.to_string())?;
    let source_path = path.unwrap_or_else(|| Path::new("."));
    let config = config_arguments.load_config(source_path)?;
    let display_path =
        path.map_or_else(|| "<stdin>".to_string(), |path| path.display().to_string());
    Ok(LoadedSource { input, config, display_path })
}

fn read_input(path: Option<&Path>) -> io::Result<String> {
    match path {
        Some(path) => std::fs::read_to_string(path),
        None => {
            if io::stdin().is_terminal() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "no input: pass a file path or pipe input via stdin",
                ));
            }
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

fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(content.as_bytes())?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn emit_diagnostics(path: &str, source: &str, diagnostics: &[tidysql::Diagnostic]) {
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

fn check_diagnostics(diagnostics: &[tidysql::Diagnostic]) -> Result<(), String> {
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
