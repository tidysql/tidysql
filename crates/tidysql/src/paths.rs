use std::path::{Component, Path, PathBuf};

pub(crate) fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    tidysql_config::normalize_path(path)
}

pub(crate) fn dedupe_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| normalize_path(path))
}

pub(crate) fn display_path(path: &Path) -> String {
    let cwd = current_dir();
    let absolute = normalize_path(path);
    if let Ok(relative) = absolute.strip_prefix(&cwd) {
        if relative.as_os_str().is_empty() {
            ".".to_string()
        } else {
            relative.display().to_string()
        }
    } else {
        absolute.display().to_string()
    }
}

pub(crate) fn contains_glob_meta(path: &Path) -> bool {
    path.to_string_lossy().chars().any(is_glob_meta_char)
}

pub(crate) fn glob_root(pattern: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(pattern);
    let mut root = if path.is_absolute() { PathBuf::new() } else { cwd.to_path_buf() };

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => root.push(prefix.as_os_str()),
            Component::RootDir => root.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => root.push(".."),
            Component::Normal(part) => {
                if part.to_string_lossy().chars().any(is_glob_meta_char) {
                    break;
                }
                root.push(part);
            }
        }
    }

    if root.as_os_str().is_empty() { cwd.to_path_buf() } else { root }
}

pub(crate) fn normalize_match_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_glob_meta_char(ch: char) -> bool {
    matches!(ch, '*' | '?' | '[' | ']' | '{' | '}')
}

pub(crate) fn collapse_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();

    let mut collapsed = Vec::new();
    for root in roots {
        if collapsed.iter().any(|existing: &PathBuf| root.starts_with(existing)) {
            continue;
        }
        collapsed.push(root);
    }

    collapsed
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::{collapse_roots, glob_root};

    #[test]
    fn glob_root_stops_on_brace_patterns() {
        let cwd = Path::new("/workspace");
        let root = glob_root("sql/{queries,migrations}/**/*.sql", cwd);
        assert_eq!(root, cwd.join("sql"));
    }

    #[test]
    fn collapse_roots_removes_nested_entries() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let roots = collapse_roots(vec![
            repo.clone(),
            repo.join("src"),
            repo.join("src").join("sql"),
            repo.join("tests"),
        ]);

        assert_eq!(roots, vec![repo]);
    }
}
