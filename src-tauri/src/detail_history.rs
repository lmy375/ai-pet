//! detail.md 自动版本历史 safety net。
//!
//! 每次 `memory_edit("update")` 把 detail_content 覆盖到 `<mem_dir>/<detail_path>`
//! 之前，把现有版本（若文件存在且非空）snapshot 到 sibling 历史目录：
//!
//!   `<mem_dir>/<detail_path>.history/<YYYYMMDD-HHMMSS>.md`
//!
//! 保留最近 `HISTORY_CAP` 份（按文件名字典序 = 时间序），旧版本删除。给 owner
//! 提供"我刚保存覆盖了，能拿回上一版吗"的安全网。
//!
//! 完全 best-effort：snapshot / trim 任一失败都不阻断 memory_edit 主路径 ——
//! owner 的本次 save 必须先成功，历史是辅助。
//!
//! 与 git / sqlite mirror 等其它持久层并存：本模块仅管 detail.md 文件的
//! `.history` 副本。task description（[task pri=...] / markers 等）走 index.yaml +
//! butler_history.log，不在本模块。

use std::fs;
use std::path::{Path, PathBuf};

/// 每条 detail.md 最多保留几份历史。5 份覆盖典型 "几小时内多次 save 想撤回"
/// 的安全网需求；过多会让 .history 目录吵杂且占磁盘。
pub const HISTORY_CAP: usize = 5;

/// pure helper：返回历史目录路径（`<detail_path>.history`）。caller 负责确保
/// detail_path 是 detail.md 文件路径（不是 dir / 空串）；空串 → "".history。
pub fn history_dir_for(detail_path: &Path) -> PathBuf {
    let mut s = detail_path.as_os_str().to_os_string();
    s.push(".history");
    PathBuf::from(s)
}

/// pure helper：把 chrono::Local::now() 格式化为文件名兼容 timestamp。
/// 字典序与时间序一致 — 便于 `.history` 内 sort + 截取。
pub fn timestamp_now() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// 在覆盖 `full_detail_path` 之前 snapshot 旧内容到 history dir。
/// 文件不存在 / 内容空 → 不 snapshot（无前一版可保）；任一 IO 失败 → 静默
/// 跳过（不阻塞 caller 的主写路径）。
pub fn snapshot_before_write(full_detail_path: &Path) {
    let prev = match fs::read_to_string(full_detail_path) {
        Ok(s) if !s.is_empty() => s,
        _ => return, // 无前版 / 读失败 → 没东西可 snapshot
    };
    let history_dir = history_dir_for(full_detail_path);
    if fs::create_dir_all(&history_dir).is_err() {
        return;
    }
    let snap_path = history_dir.join(format!("{}.md", timestamp_now()));
    // 同一秒内重复 save → 文件名撞；让后到的盖前面（与 timestamp 精度限制是
    // 一致体验，owner 不会感知 1s 内连点保存的细微差异）。
    let _ = fs::write(&snap_path, prev);
    let _ = trim_history(&history_dir, HISTORY_CAP);
}

/// 修剪 history dir 到 cap 上限。按文件名字典序排（= timestamp 序），保留尾部
/// cap 份。空目录 / 缺失目录 → no-op。
pub fn trim_history(history_dir: &Path, cap: usize) -> std::io::Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(history_dir)?
        .filter_map(|r| r.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    files.sort(); // lexicographic = timestamp ascending
    if files.len() <= cap {
        return Ok(());
    }
    let drop = files.len() - cap;
    for path in &files[..drop] {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

/// 一条历史版本的快照。给 Tauri 命令返回 owner 看 / 复制用。
#[derive(serde::Serialize)]
pub struct DetailHistoryEntry {
    /// 文件名 stem（如 `20260517-143015`）— 给 UI 显示 / 排序用。
    pub ts: String,
    /// detail.md 文件内容全文。owner 想 "拿回这份" 时 frontend copy 到剪贴板
    /// 让其粘回 textarea；不引 restore 自动写回（避免误覆盖当前 dirty 内容
    /// 的风险），由 owner 主动决策。
    pub content: String,
}

/// 列出某 detail.md 的全部 history 条目。最多 cap 份（与 HISTORY_CAP 同），
/// 倒序返回（最新在前 — UI list 第一条是 owner 最可能想拿回的"上一版"）。
/// 目录不存在 / 读失败 → 空 Vec。
pub fn list_history(full_detail_path: &Path) -> Vec<DetailHistoryEntry> {
    let dir = history_dir_for(full_detail_path);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|r| r.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    // desc 字典序 = 最新在前
    files.sort_by(|a, b| b.cmp(a));
    files
        .into_iter()
        .take(HISTORY_CAP)
        .filter_map(|p| {
            let ts = p.file_stem()?.to_string_lossy().to_string();
            let content = fs::read_to_string(&p).ok()?;
            Some(DetailHistoryEntry { ts, content })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fresh_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pet-detail-history-test-{}-{}", label, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn history_dir_appends_history_suffix() {
        let p = PathBuf::from("/tmp/foo/bar.md");
        assert_eq!(history_dir_for(&p), PathBuf::from("/tmp/foo/bar.md.history"));
    }

    #[test]
    fn snapshot_noop_when_target_missing() {
        let dir = fresh_temp_dir("noop-missing");
        let p = dir.join("missing.md");
        snapshot_before_write(&p);
        // 没 panic + history dir 也不该被建出来（前提是 "无前版不留痕"）
        let hist = history_dir_for(&p);
        assert!(!hist.exists(), "should not create empty history dir");
    }

    #[test]
    fn snapshot_noop_when_target_empty() {
        let dir = fresh_temp_dir("noop-empty");
        let p = dir.join("empty.md");
        fs::write(&p, "").unwrap();
        snapshot_before_write(&p);
        let hist = history_dir_for(&p);
        assert!(!hist.exists(), "empty file should not create snapshot");
    }

    #[test]
    fn snapshot_writes_a_versioned_file() {
        let dir = fresh_temp_dir("writes");
        let p = dir.join("task.md");
        fs::write(&p, "v1 content").unwrap();
        snapshot_before_write(&p);
        let hist = history_dir_for(&p);
        let files: Vec<_> = fs::read_dir(&hist).unwrap().collect();
        assert_eq!(files.len(), 1, "should create 1 snapshot");
        let entry = files.into_iter().next().unwrap().unwrap();
        let content = fs::read_to_string(entry.path()).unwrap();
        assert_eq!(content, "v1 content");
    }

    #[test]
    fn trim_history_keeps_cap_newest() {
        let dir = fresh_temp_dir("trim-keep");
        let hist = dir.join("h");
        fs::create_dir(&hist).unwrap();
        for name in [
            "20260101-000001.md",
            "20260101-000002.md",
            "20260101-000003.md",
            "20260101-000004.md",
        ] {
            fs::write(hist.join(name), "x").unwrap();
        }
        // cap=2 — 保留最新两份 (000003, 000004)
        trim_history(&hist, 2).unwrap();
        let remaining: Vec<String> = fs::read_dir(&hist)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.contains(&"20260101-000003.md".to_string()));
        assert!(remaining.contains(&"20260101-000004.md".to_string()));
    }

    #[test]
    fn trim_history_noop_when_under_cap() {
        let dir = fresh_temp_dir("trim-noop");
        let hist = dir.join("h");
        fs::create_dir(&hist).unwrap();
        fs::write(hist.join("a.md"), "x").unwrap();
        trim_history(&hist, 5).unwrap();
        assert_eq!(fs::read_dir(&hist).unwrap().count(), 1);
    }

    #[test]
    fn list_history_returns_desc_order_and_content() {
        let dir = fresh_temp_dir("list-desc");
        let p = dir.join("task.md");
        let hist = history_dir_for(&p);
        fs::create_dir_all(&hist).unwrap();
        fs::write(hist.join("20260101-000001.md"), "first").unwrap();
        fs::write(hist.join("20260101-000005.md"), "fifth").unwrap();
        fs::write(hist.join("20260101-000003.md"), "third").unwrap();
        let entries = list_history(&p);
        assert_eq!(entries.len(), 3);
        // newest first
        assert_eq!(entries[0].ts, "20260101-000005");
        assert_eq!(entries[0].content, "fifth");
        assert_eq!(entries[1].ts, "20260101-000003");
        assert_eq!(entries[2].ts, "20260101-000001");
    }

    #[test]
    fn list_history_returns_empty_when_dir_missing() {
        let dir = fresh_temp_dir("list-empty");
        let p = dir.join("nonexistent.md");
        let entries = list_history(&p);
        assert!(entries.is_empty());
    }

    #[test]
    fn trim_history_caps_at_history_cap() {
        // 直接 inject 文件 + 调 trim — 验证 cap 行为；snapshot_before_write 同
        // 一秒内文件名撞所以不适合反复模拟。trim 单独测语义更干净。
        let dir = fresh_temp_dir("trim-cap");
        let hist = dir.join("h");
        fs::create_dir(&hist).unwrap();
        for i in 0..(HISTORY_CAP + 2) {
            fs::write(hist.join(format!("20260101-{i:06}.md")), format!("v{i}")).unwrap();
        }
        trim_history(&hist, HISTORY_CAP).unwrap();
        let count = fs::read_dir(&hist).unwrap().count();
        assert_eq!(count, HISTORY_CAP, "trim should cap at HISTORY_CAP");
    }
}
