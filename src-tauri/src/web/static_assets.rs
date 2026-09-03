use std::path::{Path, PathBuf};

/// Find the first directory that really holds `index.html`. `None` means no
/// frontend bundle is deployed, and the caller decides whether to warn.
pub fn locate_static_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("AI_SWITCH_STATIC_DIR") {
        let path = PathBuf::from(value);
        if has_index(&path) {
            return Some(path);
        }
    }

    candidate_static_dirs()
        .into_iter()
        .find(|candidate| has_index(candidate))
}

pub fn resolve_static_dir() -> PathBuf {
    // The fallback is relative, so it only ever resolves when the process
    // working directory happens to contain `web/`. It exists to give the router
    // a PathBuf, not as a claim that the assets are there.
    locate_static_dir().unwrap_or_else(|| PathBuf::from("web"))
}

pub fn static_bundle_present(dir: &Path) -> bool {
    has_index(dir)
}

/// For the startup log: list the paths that were tried, in order, so the
/// operator can see where the bundle was expected.
pub fn static_dir_candidates_report() -> String {
    let mut lines = vec![format!(
        "  AI_SWITCH_STATIC_DIR = {}",
        std::env::var("AI_SWITCH_STATIC_DIR").unwrap_or_else(|_| "<unset>".to_string())
    )];
    for candidate in candidate_static_dirs() {
        lines.push(format!("  {}", candidate.display()));
    }
    lines.join("\n")
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

    // A request that names a file extension is an asset request, not a client
    // route. Answering it with index.html makes the browser raise a MIME error
    // that has nothing to do with the real cause (a stale or partial deploy).
    if looks_like_asset_request(trimmed) {
        return None;
    }

    // SPA fallback for unknown client routes.
    Some(static_root.join("index.html"))
}

fn looks_like_asset_request(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(_, extension)| !extension.eq_ignore_ascii_case("html"))
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

    #[test]
    fn missing_asset_requests_do_not_fall_back_to_index() {
        let dir = tempdir().unwrap();
        let web = dir.path().join("web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("index.html"), "<html></html>").unwrap();

        // Answering a .js request with index.html only makes the browser report a
        // MIME error, which says nothing about the real cause: a stale or partial
        // deployment.
        assert!(resolve_static_file(&web, "/assets/index-missing.js").is_none());
        assert!(resolve_static_file(&web, "/assets/app.css").is_none());
        // Client routes carry no extension, so those still need the fallback.
        assert!(resolve_static_file(&web, "/settings/web")
            .unwrap()
            .ends_with("index.html"));
        assert!(resolve_static_file(&web, "/index.html")
            .unwrap()
            .ends_with("index.html"));
    }

    #[test]
    fn bundle_presence_and_candidate_report_are_observable() {
        let dir = tempdir().unwrap();
        assert!(!static_bundle_present(dir.path()));
        fs::write(dir.path().join("index.html"), "<html></html>").unwrap();
        assert!(static_bundle_present(dir.path()));
        assert!(static_dir_candidates_report().contains("AI_SWITCH_STATIC_DIR"));
    }
}
