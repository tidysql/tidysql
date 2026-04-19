use std::collections::HashMap;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};

use clap::{Args, Parser, Subcommand};
use ignore::gitignore::Gitignore;

use crate::batch::{BatchCommandKind, ExecutionPlan, execution_plan, run_batch};
use crate::diagnostics::{check_diagnostics, emit_diagnostics};
use crate::paths::{display_path, normalize_path};

mod batch;
mod diagnostics;
mod lsp;
mod paths;

#[derive(Parser)]
#[command(name = "tidysql", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    global_options: GlobalConfigArgs,
}

#[derive(Args, Clone)]
struct GlobalConfigArgs {
    #[arg(short, long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    isolated: bool,
}

#[derive(Args, Clone)]
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

#[derive(Args, Clone)]
struct FormatCommand {
    #[arg(value_name = "INPUT")]
    inputs: Vec<PathBuf>,
    #[arg(long, value_name = "PATTERN")]
    glob: Vec<String>,
    #[arg(long)]
    write: bool,
    #[arg(long)]
    check: bool,
    #[arg(long, value_name = "N")]
    jobs: Option<usize>,
    #[arg(long, value_name = "PATH")]
    stdin_filename: Option<PathBuf>,
    #[command(flatten)]
    config_overrides: ConfigOverrideArgs,
}

#[derive(Args, Clone)]
struct CheckCommand {
    #[arg(value_name = "INPUT")]
    inputs: Vec<PathBuf>,
    #[arg(long, value_name = "PATTERN")]
    glob: Vec<String>,
    #[arg(long)]
    fix: bool,
    #[arg(long, value_name = "N")]
    jobs: Option<usize>,
    #[arg(long, value_name = "PATH")]
    stdin_filename: Option<PathBuf>,
    #[command(flatten)]
    config_overrides: ConfigOverrideArgs,
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
    source_path: Option<PathBuf>,
}

#[derive(Clone)]
struct ConfigArguments {
    config_path: Option<PathBuf>,
    isolated: bool,
    overrides: ConfigOverrides,
    resolver: Arc<tidysql_config::ConfigResolver>,
    ignore_matchers: Arc<IgnoreMatcherCache>,
}

#[derive(Default, Clone)]
struct ConfigOverrides {
    dialect: Option<tidysql_config::Dialect>,
    lint_levels: Vec<LintLevelOverride>,
}

#[derive(Clone)]
struct LintLevelOverride {
    lint: tidysql_config::LintName,
    level: tidysql_config::Severity,
}

#[derive(Default)]
struct IgnoreMatcherCache {
    directories: Mutex<HashMap<PathBuf, IgnoreMatchers>>,
}

#[derive(Clone)]
struct IgnoreMatchers {
    dotignore: Gitignore,
    gitignore: Gitignore,
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
        Self {
            config_path: global_options.config,
            isolated: global_options.isolated,
            overrides,
            resolver: Arc::new(tidysql_config::ConfigResolver::new()),
            ignore_matchers: Arc::new(IgnoreMatcherCache::default()),
        }
    }

    fn resolved_config(
        &self,
        source_path: &Path,
    ) -> Result<Arc<tidysql_config::ResolvedConfig>, String> {
        if self.isolated {
            return Ok(self.resolver.resolve_isolated());
        }

        match self.config_path.as_deref() {
            Some(path) => self.resolver.resolve_explicit(path).map_err(|err| err.to_string()),
            None => self.resolver.resolve(source_path).map_err(|err| err.to_string()),
        }
    }

    fn load_config(&self, source_path: &Path) -> Result<tidysql_config::Config, String> {
        let resolved = self.resolved_config(source_path)?;
        Ok(self.apply_overrides(resolved.config()))
    }

    fn apply_overrides(&self, base: &tidysql_config::Config) -> tidysql_config::Config {
        let mut config = base.clone();
        self.overrides.apply(&mut config);
        config
    }

    fn is_gitignored(&self, path: &Path, resolved_config: &tidysql_config::ResolvedConfig) -> bool {
        resolved_config.files().respect_gitignore && self.ignore_matchers.is_ignored(path)
    }

    fn is_gitignored_dir(
        &self,
        path: &Path,
        resolved_config: &tidysql_config::ResolvedConfig,
    ) -> bool {
        resolved_config.files().respect_gitignore && self.ignore_matchers.is_ignored_dir(path)
    }

    fn should_skip_directory(&self, path: &Path) -> bool {
        let Ok(resolved) = self.resolved_config(path) else {
            return false;
        };

        self.is_gitignored_dir(path, &resolved) || resolved.excludes_directory(path)
    }

    fn invalidate_resolver(&self) {
        self.resolver.invalidate();
    }
}

impl IgnoreMatcherCache {
    fn is_ignored(&self, path: &Path) -> bool {
        self.is_ignored_path(path, false)
    }

    fn is_ignored_dir(&self, path: &Path) -> bool {
        self.is_ignored_path(path, true)
    }

    fn is_ignored_path(&self, path: &Path, is_dir: bool) -> bool {
        let path = normalize_path(path);
        let Some(start) = path.parent() else {
            return false;
        };

        for dir in start.ancestors() {
            let matchers = self.matchers_for(dir);
            let dotignore = matchers.dotignore.matched_path_or_any_parents(&path, is_dir);
            if dotignore.is_ignore() {
                return true;
            }
            if dotignore.is_whitelist() {
                return false;
            }

            let gitignore = matchers.gitignore.matched_path_or_any_parents(&path, is_dir);
            if gitignore.is_ignore() {
                return true;
            }
            if gitignore.is_whitelist() {
                return false;
            }
        }

        false
    }

    fn matchers_for(&self, dir: &Path) -> IgnoreMatchers {
        let dir = normalize_path(dir);
        if let Some(matchers) = self.directories.lock().unwrap().get(&dir).cloned() {
            return matchers;
        }

        let matchers = IgnoreMatchers {
            dotignore: load_ignore_matcher(&dir, ".ignore"),
            gitignore: load_ignore_matcher(&dir, ".gitignore"),
        };
        self.directories.lock().unwrap().insert(dir, matchers.clone());
        matchers
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
    let _ = (args, global_options);
    Err("formatting is not yet implemented".to_string())
}

#[allow(dead_code)]
fn hidden_format(args: FormatCommand, global_options: GlobalConfigArgs) -> Result<(), String> {
    if args.write && args.check {
        return Err("format mode conflict: choose either --write or --check".to_string());
    }

    let plan = execution_plan(&args.inputs, &args.glob)?;
    let config_arguments = config_arguments(global_options, args.config_overrides.clone());

    match plan {
        ExecutionPlan::Stdin => format_stdin(args, &config_arguments),
        ExecutionPlan::SingleFile(path) => format_single_file(path, args, &config_arguments),
        ExecutionPlan::Batch(plan) => {
            if !args.write && !args.check {
                return Err("batch format requires either --write or --check to avoid \
                            concatenating files to stdout"
                    .to_string());
            }

            let mode = if args.write {
                BatchCommandKind::FormatWrite
            } else {
                BatchCommandKind::FormatCheck
            };
            run_batch(plan, mode, args.jobs, &config_arguments)
        }
    }
}

fn check(args: CheckCommand, global_options: GlobalConfigArgs) -> Result<(), String> {
    let plan = execution_plan(&args.inputs, &args.glob)?;
    let config_arguments = config_arguments(global_options, args.config_overrides.clone());

    match plan {
        ExecutionPlan::Stdin => check_stdin(args, &config_arguments),
        ExecutionPlan::SingleFile(path) => check_single_file(path, args, &config_arguments),
        ExecutionPlan::Batch(plan) => {
            run_batch(plan, BatchCommandKind::Check { fix: args.fix }, args.jobs, &config_arguments)
        }
    }
}

fn serve_lsp(args: LspCommand, global_options: GlobalConfigArgs) -> Result<(), String> {
    lsp::run(config_arguments(global_options, args.config_overrides))
}

fn format_stdin(args: FormatCommand, config_arguments: &ConfigArguments) -> Result<(), String> {
    if args.write {
        return Err("cannot use --write when reading from stdin".to_string());
    }

    let LoadedSource { input, config, .. } =
        load_source(None, args.stdin_filename.as_deref(), config_arguments)?;
    let formatted = tidysql::format_with_config(&input, &config).map_err(|err| err.to_string())?;

    if args.check {
        if formatted != input {
            return Err("format check failed: stdin would be reformatted".to_string());
        }
        return Ok(());
    }

    write_output(&formatted).map_err(|err| err.to_string())
}

fn format_single_file(
    path: PathBuf,
    args: FormatCommand,
    config_arguments: &ConfigArguments,
) -> Result<(), String> {
    let LoadedSource { input, config, display_path, source_path } =
        load_source(Some(&path), None, config_arguments)?;
    let formatted = tidysql::format_with_config(&input, &config).map_err(|err| err.to_string())?;

    if args.check {
        if formatted != input {
            eprintln!("{display_path}");
            return Err("format check failed: files require formatting".to_string());
        }
        return Ok(());
    }

    if args.write {
        let path = source_path.as_deref().ok_or_else(|| "missing file path".to_string())?;
        if formatted != input {
            atomic_write(path, &formatted).map_err(|err| err.to_string())?;
        }
        return Ok(());
    }

    write_output(&formatted).map_err(|err| err.to_string())
}

fn check_stdin(args: CheckCommand, config_arguments: &ConfigArguments) -> Result<(), String> {
    let LoadedSource { input, config, display_path, .. } =
        load_source(None, args.stdin_filename.as_deref(), config_arguments)?;
    let checked_source = if args.fix {
        let fixed = tidysql::fix_with_config(&input, &config).map_err(|err| err.to_string())?;
        write_output(&fixed).map_err(|err| err.to_string())?;
        fixed
    } else {
        input
    };

    let diagnostics = tidysql::check_with_config(&checked_source, &config);
    emit_diagnostics(&display_path, &checked_source, &diagnostics);
    check_diagnostics(&diagnostics)
}

fn check_single_file(
    path: PathBuf,
    args: CheckCommand,
    config_arguments: &ConfigArguments,
) -> Result<(), String> {
    let LoadedSource { input, config, display_path, source_path } =
        load_source(Some(&path), None, config_arguments)?;

    let checked_source = if args.fix {
        let fixed = tidysql::fix_with_config(&input, &config).map_err(|err| err.to_string())?;
        let path = source_path.as_deref().ok_or_else(|| "missing file path".to_string())?;
        if fixed != input {
            atomic_write(path, &fixed).map_err(|err| err.to_string())?;
        }
        fixed
    } else {
        input
    };

    let diagnostics = tidysql::check_with_config(&checked_source, &config);
    emit_diagnostics(&display_path, &checked_source, &diagnostics);
    check_diagnostics(&diagnostics)
}

fn load_source(
    path: Option<&Path>,
    stdin_filename: Option<&Path>,
    config_arguments: &ConfigArguments,
) -> Result<LoadedSource, String> {
    let input = read_input(path).map_err(|err| err.to_string())?;
    let source_path = path.map(normalize_path).or_else(|| stdin_filename.map(normalize_path));
    let lookup_path = source_path.as_deref().unwrap_or_else(|| Path::new("."));
    let config = config_arguments.load_config(lookup_path)?;
    let display_path =
        source_path.as_deref().map(display_path).unwrap_or_else(|| "<stdin>".to_string());

    Ok(LoadedSource { input, config, display_path, source_path })
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

fn load_ignore_matcher(dir: &Path, file_name: &str) -> Gitignore {
    load_ignore_matcher_with_reporter(dir, file_name, |message| eprintln!("{message}"))
}

fn load_ignore_matcher_with_reporter(
    dir: &Path,
    file_name: &str,
    mut warn: impl FnMut(String),
) -> Gitignore {
    let path = dir.join(file_name);
    if !path.is_file() {
        return Gitignore::empty();
    }

    let (matcher, error) = Gitignore::new(&path);
    if let Some(error) = error {
        warn(format!("warning: failed to parse ignore file {}: {error}", path.display()));
    }
    matcher
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        ConfigOverrideArgs, FormatCommand, GlobalConfigArgs, format,
        load_ignore_matcher_with_reporter,
    };

    #[test]
    fn format_command_is_disabled() {
        let result = format(
            FormatCommand {
                inputs: Vec::new(),
                glob: Vec::new(),
                write: false,
                check: false,
                jobs: None,
                stdin_filename: None,
                config_overrides: ConfigOverrideArgs {
                    dialect: None,
                    allow: Vec::new(),
                    warn: Vec::new(),
                    deny: Vec::new(),
                },
            },
            GlobalConfigArgs { config: None, isolated: false },
        );

        assert_eq!(result.unwrap_err(), "formatting is not yet implemented");
    }

    #[test]
    fn malformed_gitignore_warns_and_keeps_valid_patterns() {
        let dir = tempdir().unwrap();
        let ignore_path = dir.path().join(".gitignore");
        std::fs::write(&ignore_path, "ignored.sql\n\\\n").unwrap();

        let mut warnings = Vec::new();
        let matcher = load_ignore_matcher_with_reporter(dir.path(), ".gitignore", |message| {
            warnings.push(message)
        });

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("warning: failed to parse ignore file"));
        assert!(warnings[0].contains(ignore_path.to_string_lossy().as_ref()));

        let ignored = dir.path().join("ignored.sql");
        let kept = dir.path().join("kept.sql");
        assert!(matcher.matched_path_or_any_parents(&ignored, false).is_ignore());
        assert!(!matcher.matched_path_or_any_parents(&kept, false).is_ignore());
    }
}
