//! The agent's working directory: where `bash` runs when a tool call doesn't
//! name a directory of its own, and what the system prompt tells the model
//! "当前目录" means.
//!
//! Process-global and deliberately NOT persisted — the two interfaces want
//! different defaults, and config.yaml is shared by both. The GUI takes the
//! lazy default below (`$HOME`); the CLI overrides it at startup with the
//! directory it was launched from, the way any terminal tool behaves. The
//! picker in the panel's session rail moves it for the running GUI only.

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// `$HOME`, falling back to the root when it can't be resolved. This is the
/// GUI's working directory; `set` is what moves it.
fn default_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn cell() -> &'static RwLock<PathBuf> {
    static DIR: OnceLock<RwLock<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| RwLock::new(default_dir()))
}

/// The current working directory.
pub fn get() -> PathBuf {
    cell().read().map(|d| d.clone()).unwrap_or_else(|_| default_dir())
}

/// The current working directory as a string, for the prompt and the UI.
pub fn get_string() -> String {
    get().to_string_lossy().to_string()
}

/// Move the working directory. Rejects anything that isn't an existing
/// directory, so a bad value can't silently break every later `bash` call.
/// Returns the accepted path.
pub fn set(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_dir() {
        return Err(format!("不是有效目录: {}", path.display()));
    }
    // Resolve symlinks/`..` so the prompt shows one canonical path.
    let path = path.canonicalize().unwrap_or(path);
    *cell().write().map_err(|e| format!("工作目录被锁住了: {e}"))? = path.clone();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path that isn't a directory must be refused rather than stored — the
    /// working directory is handed to `Command::current_dir`, where a bad value
    /// fails every spawn afterwards.
    #[test]
    fn rejects_a_non_directory() {
        let before = get();
        let file = std::env::current_exe().expect("test binary path");
        assert!(set(&file.to_string_lossy()).is_err());
        assert!(set("/definitely/not/here").is_err());
        assert_eq!(get(), before);
    }
}
