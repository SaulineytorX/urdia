#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
use std::collections::BTreeMap;
use std::path::Path;

#[cfg(target_os = "linux")]
const MAX_VISITED_ENTRIES: usize = 4096;
#[cfg(target_os = "linux")]
const MAX_SELECTED_FILES: usize = 128;
#[cfg(target_os = "linux")]
const MAX_PATH_DEPTH: usize = 8;
#[cfg(target_os = "linux")]
const MAX_SEGMENT_BYTES: usize = 128;
#[cfg(target_os = "linux")]
const MAX_REL_PATH_BYTES: usize = 512;
#[cfg(target_os = "linux")]
const MAX_FILE_BYTES: usize = 1024 * 1024;
#[cfg(target_os = "linux")]
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

#[cfg(target_os = "linux")]
const ALLOWED_EXTENSIONS: &[&str] = &[
    "html", "css", "js", "svg", "png", "jpg", "jpeg", "ico", "woff", "woff2",
];

#[cfg(target_os = "linux")]
const IGNORED_DIRS: &[&str] = &["node_modules", "vendor", "venv", "__pycache__", "target"];

/// Reports whether local workspace preview capture is supported on this platform.
pub fn available() -> bool {
    cfg!(target_os = "linux")
}

/// Captures an immutable byte copy of static preview assets from `workspace`.
///
/// This captures the current files on disk across two identical passes and does not
/// claim atomic filesystem isolation or exact post-task authorship.
#[cfg(target_os = "linux")]
pub fn snapshot(workspace: &Path) -> Option<urdia_ui::ResultPreviewBundle> {
    let files = capture_verified_map(workspace)?;
    urdia_ui::ResultPreviewBundle::new(files).ok()
}

#[cfg(not(target_os = "linux"))]
pub fn snapshot(_workspace: &Path) -> Option<urdia_ui::ResultPreviewBundle> {
    None
}

#[cfg(target_os = "linux")]
fn capture_verified_map(workspace: &Path) -> Option<BTreeMap<String, Vec<u8>>> {
    let first = capture_once(workspace)?;
    let second = capture_once(workspace)?;
    if first != second {
        return None;
    }
    Some(first)
}

#[cfg(target_os = "linux")]
struct TraversalState {
    visited_entries: usize,
    selected_files: usize,
    total_bytes: usize,
    files: BTreeMap<String, Vec<u8>>,
}

#[cfg(target_os = "linux")]
fn capture_once(workspace: &Path) -> Option<BTreeMap<String, Vec<u8>>> {
    if workspace.as_os_str().is_empty() {
        return None;
    }

    let root_dir = open_workspace_root(workspace)?;
    let mut state = TraversalState {
        visited_entries: 0,
        selected_files: 0,
        total_bytes: 0,
        files: BTreeMap::new(),
    };

    traverse_dir(&root_dir, "", 1, &mut state)?;

    if !state.files.contains_key("index.html") {
        return None;
    }

    Some(state.files)
}

#[cfg(target_os = "linux")]
fn traverse_dir(
    dir_file: &std::fs::File,
    rel_prefix: &str,
    depth: usize,
    state: &mut TraversalState,
) -> Option<()> {
    use std::os::fd::AsRawFd;
    use std::path::PathBuf;

    if depth > MAX_PATH_DEPTH {
        return None;
    }

    let fd_dir_path = PathBuf::from(format!("/proc/self/fd/{}", dir_file.as_raw_fd()));
    let entries = std::fs::read_dir(&fd_dir_path).ok()?;

    for entry_res in entries {
        let entry = entry_res.ok()?;
        state.visited_entries = state.visited_entries.checked_add(1)?;
        if state.visited_entries > MAX_VISITED_ENTRIES {
            return None;
        }

        let raw_name = entry.file_name();
        let name = raw_name.to_str()?;
        if !is_valid_segment(name) {
            return None;
        }

        if name.starts_with('.') {
            continue;
        }

        if IGNORED_DIRS.contains(&name) {
            continue;
        }

        let file_type = entry.file_type().ok()?;
        if file_type.is_symlink() {
            return None;
        }

        let rel_path = if rel_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{rel_prefix}/{name}")
        };
        if rel_path.len() > MAX_REL_PATH_BYTES {
            return None;
        }

        let child_fd_path =
            PathBuf::from(format!("/proc/self/fd/{}/{}", dir_file.as_raw_fd(), name));

        if file_type.is_dir() {
            if depth >= MAX_PATH_DEPTH {
                return None;
            }
            let child_dir = open_dir_nofollow(&child_fd_path)?;
            traverse_dir(&child_dir, &rel_path, depth + 1, state)?;
            continue;
        }

        if !has_allowed_extension(name) {
            continue;
        }

        if !file_type.is_file() {
            return None;
        }

        let bytes = read_regular_file_nofollow(&child_fd_path)?;
        state.selected_files = state.selected_files.checked_add(1)?;
        if state.selected_files > MAX_SELECTED_FILES {
            return None;
        }

        state.total_bytes = state.total_bytes.checked_add(bytes.len())?;
        if state.total_bytes > MAX_TOTAL_BYTES {
            return None;
        }

        if state.files.insert(rel_path, bytes).is_some() {
            return None;
        }
    }

    Some(())
}

#[cfg(target_os = "linux")]
fn open_workspace_root(path: &Path) -> Option<std::fs::File> {
    use std::os::fd::AsRawFd;
    use std::path::{Component, PathBuf};
    if !path.is_absolute() {
        return None;
    }
    let mut directory = open_dir_nofollow(Path::new("/"))?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                let anchored =
                    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd())).join(name);
                directory = open_dir_nofollow(&anchored)?;
            }
            _ => return None,
        }
    }
    Some(directory)
}

#[cfg(target_os = "linux")]
fn open_dir_nofollow(path: &Path) -> Option<std::fs::File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_dir() || meta.nlink() == 0 {
        return None;
    }
    Some(file)
}

#[cfg(target_os = "linux")]
fn read_regular_file_nofollow(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .ok()?;

    let before = file.metadata().ok()?;
    if !before.file_type().is_file() || before.nlink() != 1 {
        return None;
    }

    let expected_len = usize::try_from(before.len()).ok()?;
    if expected_len > MAX_FILE_BYTES {
        return None;
    }

    let max_read = u64::try_from(MAX_FILE_BYTES + 1).ok()?;
    let mut buf = Vec::with_capacity(expected_len);
    (&file).take(max_read).read_to_end(&mut buf).ok()?;

    if buf.len() != expected_len || buf.len() > MAX_FILE_BYTES {
        return None;
    }

    let after = file.metadata().ok()?;
    if !after.file_type().is_file()
        || after.nlink() != 1
        || after.len() != before.len()
        || after.dev() != before.dev()
        || after.ino() != before.ino()
        || after.mtime() != before.mtime()
        || after.mtime_nsec() != before.mtime_nsec()
        || after.ctime() != before.ctime()
        || after.ctime_nsec() != before.ctime_nsec()
    {
        return None;
    }

    Some(buf)
}

#[cfg(target_os = "linux")]
fn is_valid_segment(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_SEGMENT_BYTES || name == "." || name == ".." {
        return false;
    }
    !name
        .chars()
        .any(|ch| ch == '/' || ch == '\\' || ch.is_control())
}

#[cfg(target_os = "linux")]
fn has_allowed_extension(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() || ext.is_empty() {
        return false;
    }
    ALLOWED_EXTENSIONS
        .iter()
        .any(|allowed| ext.eq_ignore_ascii_case(allowed))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    #[test]
    fn accepts_regular_nested_assets_and_ignores_json_and_hidden() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();

        fs::write(root.join("index.html"), b"<!doctype html><title>ok</title>").unwrap();
        fs::create_dir(root.join("assets")).unwrap();
        fs::write(root.join("assets/app.js"), b"console.log('ready');").unwrap();
        fs::write(root.join("assets/style.css"), b"body { margin: 0; }").unwrap();
        fs::write(root.join("config.json"), br#"{"secret":true}"#).unwrap();
        fs::write(root.join(".env.html"), b"hidden").unwrap();

        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/index.html"), b"ignored").unwrap();

        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules/pkg.js"), b"ignored").unwrap();

        let captured = capture_verified_map(root).expect("capture should succeed");
        assert_eq!(captured.len(), 3);
        assert!(captured.contains_key("index.html"));
        assert!(captured.contains_key("assets/app.js"));
        assert!(captured.contains_key("assets/style.css"));
        assert!(!captured.contains_key("config.json"));
    }

    #[test]
    fn refuses_missing_index_html() {
        let tmp = TempDir::new().expect("tempdir");
        fs::write(tmp.path().join("app.js"), b"console.log(1);").unwrap();
        assert!(capture_verified_map(tmp.path()).is_none());
    }

    #[test]
    fn refuses_symlinks_for_root_dir_and_selected_file() {
        let tmp = TempDir::new().expect("tempdir");
        let real_root = tmp.path().join("real");
        fs::create_dir(&real_root).unwrap();
        fs::write(real_root.join("index.html"), b"<h1>hi</h1>").unwrap();

        let symlink_root = tmp.path().join("root_link");
        symlink(&real_root, &symlink_root).unwrap();
        assert!(capture_verified_map(&symlink_root).is_none());

        let outside = tmp.path().join("outside.js");
        fs::write(&outside, b"alert(1);").unwrap();
        symlink(&outside, real_root.join("leak.js")).unwrap();
        assert!(capture_verified_map(&real_root).is_none());
        fs::remove_file(real_root.join("leak.js")).unwrap();

        let outside_dir = tmp.path().join("outside_dir");
        fs::create_dir(&outside_dir).unwrap();
        fs::write(outside_dir.join("part.css"), b"a{}").unwrap();
        symlink(&outside_dir, real_root.join("linked_dir")).unwrap();
        assert!(capture_verified_map(&real_root).is_none());
    }

    #[test]
    fn refuses_hardlinked_selected_file() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        let index = root.join("index.html");
        fs::write(&index, b"<!doctype html>").unwrap();
        fs::hard_link(&index, root.join("extra.txt")).unwrap();

        assert!(capture_verified_map(root).is_none());
    }

    #[test]
    fn refuses_oversized_selected_file() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        fs::write(root.join("index.html"), b"<!doctype html>").unwrap();
        fs::write(root.join("huge.png"), vec![0_u8; MAX_FILE_BYTES + 1]).unwrap();

        assert!(capture_verified_map(root).is_none());
    }
    #[test]
    fn refuses_symlink_ancestor_and_nonregular_source_without_blocking() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("real");
        fs::create_dir_all(real.join("app")).unwrap();
        fs::write(real.join("app/index.html"), "<h1>App</h1>").unwrap();
        symlink(&real, tmp.path().join("alias")).unwrap();
        assert!(snapshot(&tmp.path().join("alias/app")).is_none());
        let _socket = std::os::unix::net::UnixListener::bind(real.join("app/socket.js")).unwrap();
        assert!(snapshot(&real.join("app")).is_none());
    }
    #[test]
    fn captures_stable_bytes_and_rejects_excess_file_count() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("index.html"), "original").unwrap();
        let captured = capture_verified_map(tmp.path()).unwrap();
        fs::write(tmp.path().join("index.html"), "changed").unwrap();
        assert_eq!(captured["index.html"], b"original");
        for n in 0..128 {
            fs::write(tmp.path().join(format!("asset-{n}.js")), "").unwrap();
        }
        assert!(snapshot(tmp.path()).is_none());
    }
}
