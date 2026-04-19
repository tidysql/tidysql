use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub(crate) fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { current_dir().join(path) };
    lexical_normalize_path(&absolute)
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut prefix = None;
    let mut has_root = false;
    let mut parts: Vec<OsString> = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => has_root = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = parts.last()
                    && last != ".."
                {
                    parts.pop();
                    continue;
                }
                if !has_root {
                    parts.push("..".into());
                }
            }
            Component::Normal(part) => parts.push(part.to_os_string()),
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if has_root {
        normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR));
    }
    for part in parts {
        normalized.push(part);
    }

    if normalized.as_os_str().is_empty() { PathBuf::from(".") } else { normalized }
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
