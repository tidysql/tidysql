use std::collections::HashSet;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use super::{BatchInputPlan, ExecutionPlan, FileJob, FileOutcome, FileResult};
use crate::ConfigArguments;
use crate::paths::{
    collapse_roots, contains_glob_meta, current_dir, dedupe_key, display_path, glob_root,
    normalize_match_string, normalize_path,
};

#[derive(Clone)]
struct BatchScope {
    directory_roots: Vec<PathBuf>,
    input_globs: CompiledInputGlobs,
}

#[derive(Clone)]
struct CompiledInputGlobs {
    set: GlobSet,
    has_patterns: bool,
}

pub(crate) fn execution_plan(
    inputs: &[PathBuf],
    globs: &[String],
) -> Result<ExecutionPlan, String> {
    if inputs.is_empty() && globs.is_empty() {
        if io::stdin().is_terminal() {
            return Err(
                "no input: pass files/directories/globs or pipe input via stdin".to_string()
            );
        }
        return Ok(ExecutionPlan::Stdin);
    }

    let mut explicit_files = Vec::new();
    let mut directory_roots = Vec::new();
    let mut glob_patterns = globs.to_vec();

    for input in inputs {
        if input.is_file() {
            explicit_files.push(normalize_path(input));
        } else if input.is_dir() {
            directory_roots.push(normalize_path(input));
        } else if contains_glob_meta(input) {
            glob_patterns.push(input.to_string_lossy().into_owned());
        } else {
            return Err(format!("input does not exist: {}", input.display()));
        }
    }

    if explicit_files.len() == 1 && directory_roots.is_empty() && glob_patterns.is_empty() {
        return Ok(ExecutionPlan::SingleFile(explicit_files.remove(0)));
    }

    Ok(ExecutionPlan::Batch(BatchInputPlan { explicit_files, directory_roots, glob_patterns }))
}

pub(super) fn discover_jobs_with<F, G>(
    plan: BatchInputPlan,
    config_arguments: &ConfigArguments,
    mut on_job: F,
    mut on_result: G,
) -> Result<(), String>
where
    F: FnMut(FileJob) -> bool,
    G: FnMut(FileResult) -> bool,
{
    let mut seen = HashSet::new();
    let mut seq = 0usize;
    let scope = BatchScope::new(&plan)?;

    for path in &plan.explicit_files {
        let dedupe = dedupe_key(path);
        if !seen.insert(dedupe) {
            continue;
        }

        match config_arguments.resolved_config(path) {
            Ok(resolved) => {
                if !resolved.matches_explicit_file(path) {
                    continue;
                }

                let should_continue = on_job(FileJob {
                    seq,
                    path: path.clone(),
                    display_path: display_path(path),
                    resolved_config: resolved,
                });
                seq += 1;
                if !should_continue {
                    return Ok(());
                }
            }
            Err(message) => {
                let should_continue = on_result(FileResult {
                    seq,
                    display_path: display_path(path),
                    outcome: FileOutcome::Error(message),
                });
                seq += 1;
                if !should_continue {
                    return Ok(());
                }
            }
        }
    }

    if scope.directory_roots.is_empty() {
        return Ok(());
    }

    let roots = collapse_roots(scope.directory_roots.clone());
    let mut builder = WalkBuilder::new(&roots[0]);
    for root in roots.iter().skip(1) {
        builder.add(root);
    }
    builder.git_ignore(false);
    builder.ignore(false);
    builder.git_global(false);
    builder.git_exclude(false);
    builder.parents(false);
    builder.sort_by_file_path(|left, right| left.cmp(right));

    let filter_config = config_arguments.clone();
    builder.filter_entry(move |entry| {
        entry.file_type().map(|kind| !kind.is_dir()).unwrap_or(true)
            || !filter_config.should_skip_directory(entry.path())
    });

    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let should_continue = on_result(FileResult {
                    seq,
                    display_path: "<discovery>".to_string(),
                    outcome: FileOutcome::Error(error.to_string()),
                });
                seq += 1;
                if !should_continue {
                    return Ok(());
                }
                continue;
            }
        };

        if !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
            continue;
        }

        let path = normalize_path(entry.path());
        if !scope.matches(&path) {
            continue;
        }

        let dedupe = dedupe_key(&path);
        if !seen.insert(dedupe) {
            continue;
        }

        match config_arguments.resolved_config(&path) {
            Ok(resolved) => {
                if config_arguments.is_gitignored(&path, &resolved)
                    || !resolved.matches_discovered_file(&path)
                {
                    continue;
                }

                let should_continue = on_job(FileJob {
                    seq,
                    path: path.clone(),
                    display_path: display_path(&path),
                    resolved_config: resolved,
                });
                seq += 1;
                if !should_continue {
                    return Ok(());
                }
            }
            Err(message) => {
                let should_continue = on_result(FileResult {
                    seq,
                    display_path: display_path(&path),
                    outcome: FileOutcome::Error(message),
                });
                seq += 1;
                if !should_continue {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

impl BatchScope {
    fn new(plan: &BatchInputPlan) -> Result<Self, String> {
        let cwd = current_dir();
        let mut roots = plan.directory_roots.clone();
        for pattern in &plan.glob_patterns {
            let root = glob_root(pattern, &cwd);
            if root.exists() {
                roots.push(root);
            }
        }

        Ok(Self {
            directory_roots: roots,
            input_globs: CompiledInputGlobs::compile(&plan.glob_patterns, &cwd)?,
        })
    }

    fn matches(&self, path: &Path) -> bool {
        self.matches_root(path) || self.input_globs.is_match(path)
    }

    fn matches_root(&self, path: &Path) -> bool {
        self.directory_roots.iter().any(|root| path.starts_with(root))
    }
}

impl CompiledInputGlobs {
    fn compile(patterns: &[String], cwd: &Path) -> Result<Self, String> {
        let mut builder = GlobSetBuilder::new();

        for pattern in patterns {
            let pattern_path = Path::new(pattern);
            let absolute_pattern = if pattern_path.is_absolute() {
                normalize_match_string(pattern_path)
            } else {
                normalize_match_string(&cwd.join(pattern_path))
            };
            let glob = Glob::new(&absolute_pattern)
                .map_err(|error| format!("invalid glob pattern `{pattern}`: {error}"))?;
            builder.add(glob);
        }

        let set =
            builder.build().map_err(|error| format!("failed to compile input globs: {error}"))?;

        Ok(Self { set, has_patterns: !patterns.is_empty() })
    }

    fn is_match(&self, path: &Path) -> bool {
        self.has_patterns && self.set.is_match(normalize_match_string(path))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        BatchInputPlan, ExecutionPlan, FileJob, FileResult, discover_jobs_with, execution_plan,
    };
    use crate::paths::normalize_path;
    use crate::{
        CheckOverrideArgs, ConfigArguments, ConfigOverrides, CoreOverrideArgs, GlobalConfigArgs,
        LintOverrideArgs,
    };

    fn discover_jobs(
        plan: BatchInputPlan,
        config_arguments: &ConfigArguments,
    ) -> Result<(Vec<FileJob>, Vec<FileResult>), String> {
        let mut jobs = Vec::new();
        let mut results = Vec::new();
        discover_jobs_with(
            plan,
            config_arguments,
            |job| {
                jobs.push(job);
                true
            },
            |result| {
                results.push(result);
                true
            },
        )?;

        Ok((jobs, results))
    }

    #[test]
    fn execution_plan_prefers_single_existing_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("query.sql");
        fs::write(&file, "select 1").unwrap();

        let plan = execution_plan(std::slice::from_ref(&file), &[]).unwrap();
        match plan {
            ExecutionPlan::SingleFile(path) => assert_eq!(path, normalize_path(&file)),
            ExecutionPlan::Stdin | ExecutionPlan::Batch(_) => panic!("expected single file plan"),
        }
    }

    #[test]
    fn execution_plan_promotes_dirs_and_globs_to_batch() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("sql");
        fs::create_dir_all(&nested).unwrap();

        let plan = execution_plan(&[nested], &["**/*.sql".to_string()]).unwrap();
        match plan {
            ExecutionPlan::Batch(plan) => {
                assert_eq!(plan.directory_roots.len(), 1);
                assert_eq!(plan.glob_patterns, vec!["**/*.sql".to_string()]);
            }
            ExecutionPlan::Stdin | ExecutionPlan::SingleFile(_) => panic!("expected batch plan"),
        }
    }

    #[test]
    fn discover_jobs_respects_gitignore_by_default() {
        let dir = tempdir().unwrap();
        let sql_dir = dir.path().join("sql");
        fs::create_dir_all(&sql_dir).unwrap();
        fs::write(sql_dir.join(".gitignore"), "ignored.sql\n").unwrap();
        fs::write(sql_dir.join("ignored.sql"), "select 1\n").unwrap();

        let plan = BatchInputPlan {
            explicit_files: Vec::new(),
            directory_roots: vec![normalize_path(&sql_dir)],
            glob_patterns: Vec::new(),
        };
        let config_arguments = ConfigArguments::from_cli_arguments(
            GlobalConfigArgs { config: None, isolated: false },
            ConfigOverrides::from(CheckOverrideArgs {
                core: CoreOverrideArgs { dialect: None },
                lints: LintOverrideArgs { allow: Vec::new(), warn: Vec::new(), deny: Vec::new() },
            }),
        );

        let (jobs, results) = discover_jobs(plan, &config_arguments).unwrap();
        assert!(results.is_empty());
        assert!(jobs.is_empty());
    }

    #[test]
    fn discover_jobs_can_disable_gitignore_per_config() {
        let dir = tempdir().unwrap();
        let sql_dir = dir.path().join("sql");
        fs::create_dir_all(&sql_dir).unwrap();
        fs::write(sql_dir.join(".gitignore"), "ignored.sql\n").unwrap();
        fs::write(
            sql_dir.join("tidysql.toml"),
            r#"
[files]
respect_gitignore = false
"#,
        )
        .unwrap();
        let ignored = sql_dir.join("ignored.sql");
        fs::write(&ignored, "select 1\n").unwrap();

        let plan = BatchInputPlan {
            explicit_files: Vec::new(),
            directory_roots: vec![normalize_path(&sql_dir)],
            glob_patterns: Vec::new(),
        };
        let config_arguments = ConfigArguments::from_cli_arguments(
            GlobalConfigArgs { config: None, isolated: false },
            ConfigOverrides::default(),
        );

        let (jobs, results) = discover_jobs(plan, &config_arguments).unwrap();
        assert!(results.is_empty());
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].path, normalize_path(&ignored));
    }

    #[test]
    fn default_excluded_directories_are_pruned_before_processing() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let target_dir = repo.join("target");
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(target_dir.join("ignored.sql"), "select 1\n").unwrap();

        let config_arguments = ConfigArguments::from_cli_arguments(
            GlobalConfigArgs { config: None, isolated: false },
            ConfigOverrides::default(),
        );

        assert!(config_arguments.should_skip_directory(&target_dir));

        let plan = BatchInputPlan {
            explicit_files: Vec::new(),
            directory_roots: vec![normalize_path(&repo)],
            glob_patterns: Vec::new(),
        };

        let (jobs, results) = discover_jobs(plan, &config_arguments).unwrap();
        assert!(results.is_empty());
        assert!(jobs.is_empty());
    }
}
