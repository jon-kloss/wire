use include_dir::{include_dir, Dir};
use std::path::{Component, Path, PathBuf};

/// Seed files embedded into the binary at compile time. Written into each new
/// session sandbox so the playground works without any external assets.
static SEEDS: Dir = include_dir!("$CARGO_MANIFEST_DIR/seeds");

/// Create `root` and extract every embedded seed file into it, substituting the
/// demo API base URL into text files (used by the seeded environment configs).
pub fn seed_sandbox(root: &Path, demo_base_url: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    write_dir(&SEEDS, root, demo_base_url)
}

fn write_dir(dir: &Dir, root: &Path, demo_base_url: &str) -> std::io::Result<()> {
    for sub in dir.dirs() {
        std::fs::create_dir_all(root.join(sub.path()))?;
        write_dir(sub, root, demo_base_url)?;
    }
    for file in dir.files() {
        let target = root.join(file.path());
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = file.contents();
        match std::str::from_utf8(contents) {
            Ok(text) => {
                let replaced = text.replace("__WIRE_DEMO_BASE__", demo_base_url);
                std::fs::write(&target, replaced)?;
            }
            Err(_) => std::fs::write(&target, contents)?,
        }
    }
    Ok(())
}

/// Resolve a client-supplied path against the session sandbox, rejecting any
/// path that escapes it. This is the filesystem half of the demo's isolation:
/// a client can only ever read or write inside its own sandbox.
pub fn resolve_in_sandbox(sandbox: &Path, requested: &str) -> Result<PathBuf, String> {
    let requested_path = Path::new(requested);
    let joined = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        sandbox.join(requested_path)
    };
    let normalized = normalize(&joined);
    if normalized.starts_with(sandbox) {
        Ok(normalized)
    } else {
        Err(format!("Path escapes the session sandbox: {requested}"))
    }
}

/// Lexically normalize a path, resolving `.` and `..` without touching the
/// filesystem (so it works for paths that don't exist yet, e.g. new files).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confines_paths_to_sandbox() {
        let sandbox = Path::new("/tmp/wire-web/abc");
        assert!(resolve_in_sandbox(sandbox, "collections/petstore/.wire").is_ok());
        assert!(resolve_in_sandbox(sandbox, "/tmp/wire-web/abc/x").is_ok());
        assert!(resolve_in_sandbox(sandbox, "../../etc/passwd").is_err());
        assert!(resolve_in_sandbox(sandbox, "/etc/passwd").is_err());
    }
}
