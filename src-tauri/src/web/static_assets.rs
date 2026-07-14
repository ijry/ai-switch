use std::path::{Path, PathBuf};

pub fn resolve_static_dir() -> PathBuf {
    if let Ok(value) = std::env::var("AI_SWITCH_STATIC_DIR") {
        let path = PathBuf::from(value);
        if has_index(&path) {
            return path;
        }
    }

    for candidate in candidate_static_dirs() {
        if has_index(&candidate) {
            return candidate;
        }
    }

    // Stable fallback used only for diagnostics when no assets are present.
    PathBuf::from("web")
}

fn candidate_static_dirs() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.extend([
                exe_dir.join("web"),
                exe_dir.join("dist"),
                exe_dir.join("resources").join("web"),
                exe_dir.join("_up_").join("web"),
                exe_dir.join("..").join("web"),
                exe_dir.join("..").join("dist"),
                exe_dir.join("..").join("..").join("dist"),
            ]);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.extend([
            cwd.join("web"),
            cwd.join("dist"),
            cwd.join("..").join("dist"),
            cwd.join("src-tauri").join("..").join("dist"),
        ]);
    }

    candidates
}

fn has_index(path: &Path) -> bool {
    path.join("index.html").is_file()
}

pub fn resolve_static_file(static_dir: &Path, request_path: &str) -> Option<PathBuf> {
    if !has_index(static_dir) {
        return None;
    }

    let trimmed = request_path.trim_start_matches('/');
    let requested = if trimmed.is_empty() {
        static_dir.join("index.html")
    } else {
        static_dir.join(trimmed)
    };

    let Ok(static_root) = static_dir.canonicalize() else {
        return Some(static_dir.join("index.html"));
    };

    if let Ok(canonical) = requested.canonicalize() {
        if canonical.starts_with(&static_root) && canonical.is_file() {
            return Some(canonical);
        }
    }

    // SPA fallback for unknown client routes.
    Some(static_root.join("index.html"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn prefers_directory_with_index_html() {
        let dir = tempdir().unwrap();
        let web = dir.path().join("web");
        fs::create_dir_all(web.join("assets")).unwrap();
        fs::write(web.join("index.html"), "<html></html>").unwrap();
        fs::write(web.join("assets").join("app.js"), "console.log(1)").unwrap();

        assert!(has_index(&web));
        assert!(!has_index(dir.path()));

        let index = resolve_static_file(&web, "/").unwrap();
        assert!(index.ends_with("index.html"));

        let asset = resolve_static_file(&web, "/assets/app.js").unwrap();
        assert!(asset.ends_with("app.js"));
    }
}
