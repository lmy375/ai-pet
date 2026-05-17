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

/// 占盘统计返回结构：所有 `.history` 目录的总字节数 + 文件数 + 目录数。
/// 给 PanelDebug「🗄 detail .history 占盘」chip 用 — 让 owner 看到 safety
/// net 的磁盘成本，决策"该不该清 / 调小 HISTORY_CAP"。
#[derive(Debug, Default, serde::Serialize)]
pub struct DetailHistoryDiskUsage {
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
}

/// 递归扫 `mem_dir`，累加所有以 `.history` 结尾的目录内文件大小。返回
/// `(total_bytes, file_count, dir_count)`。`mem_dir` 不存在 / 不可读 →
/// 全 0。单 file IO 失败容忍（个别 file metadata 读失败不阻断）。
pub fn scan_history_disk_usage(mem_dir: &Path) -> DetailHistoryDiskUsage {
    let mut out = DetailHistoryDiskUsage::default();
    if !mem_dir.exists() {
        return out;
    }
    let mut stack: Vec<PathBuf> = vec![mem_dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                // 该 dir 是 .history 目录吗？匹配 dir name 后缀 `.history`
                let is_history_dir = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|n| n.ends_with(".history"))
                    .unwrap_or(false);
                if is_history_dir {
                    out.dir_count += 1;
                    // 累加目录内所有文件大小（一层；history dir 内不嵌套）
                    if let Ok(inner) = fs::read_dir(&path) {
                        for ient in inner.flatten() {
                            let Ok(ift) = ient.file_type() else {
                                continue;
                            };
                            if ift.is_file() {
                                if let Ok(meta) = ient.metadata() {
                                    out.total_bytes = out.total_bytes.saturating_add(meta.len());
                                    out.file_count += 1;
                                }
                            }
                        }
                    }
                    // 不再递归进 history dir
                } else {
                    // 普通子目录继续递归找 .history
                    stack.push(path);
                }
            }
        }
    }
    out
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
    fn scan_disk_usage_empty_dir_returns_zero() {
        let dir = fresh_temp_dir("disk-empty");
        let u = scan_history_disk_usage(&dir);
        assert_eq!(u.total_bytes, 0);
        assert_eq!(u.file_count, 0);
        assert_eq!(u.dir_count, 0);
    }

    #[test]
    fn scan_disk_usage_missing_dir_returns_zero() {
        let dir = fresh_temp_dir("disk-missing");
        let missing = dir.join("does-not-exist");
        let u = scan_history_disk_usage(&missing);
        assert_eq!(u.total_bytes, 0);
        assert_eq!(u.file_count, 0);
        assert_eq!(u.dir_count, 0);
    }

    #[test]
    fn scan_disk_usage_aggregates_history_dirs_only() {
        let dir = fresh_temp_dir("disk-agg");
        // 建模拟 mem_dir 结构：
        //   <mem>/butler_tasks/foo.md.history/<ts1>.md (10B)
        //                                     /<ts2>.md (20B)
        //   <mem>/butler_tasks/foo.md          (5B 但不算 — 不是 .history)
        //   <mem>/general/note.md.history/<ts>.md (8B)
        //   <mem>/general/note.md              (3B 不算)
        let cat1 = dir.join("butler_tasks");
        fs::create_dir(&cat1).unwrap();
        let hist1 = cat1.join("foo.md.history");
        fs::create_dir(&hist1).unwrap();
        fs::write(hist1.join("20260101-000001.md"), "x".repeat(10)).unwrap();
        fs::write(hist1.join("20260101-000002.md"), "x".repeat(20)).unwrap();
        fs::write(cat1.join("foo.md"), "x".repeat(5)).unwrap();

        let cat2 = dir.join("general");
        fs::create_dir(&cat2).unwrap();
        let hist2 = cat2.join("note.md.history");
        fs::create_dir(&hist2).unwrap();
        fs::write(hist2.join("20260101-000003.md"), "x".repeat(8)).unwrap();
        fs::write(cat2.join("note.md"), "x".repeat(3)).unwrap();

        let u = scan_history_disk_usage(&dir);
        assert_eq!(u.total_bytes, 10 + 20 + 8, "should only sum .history files");
        assert_eq!(u.file_count, 3, "should count 3 snapshot files");
        assert_eq!(u.dir_count, 2, "two .history dirs");
    }

    #[test]
    fn scan_disk_usage_does_not_recurse_into_history_dir() {
        // 防御：若 .history 内某天有 sub-dir 不该被进一步遍历（避免误把
        // 不相关 file 算进 .history 体积）。当前实现 inner read_dir 仅
        // is_file() 判断 — sub-dir 内文件不会被加。
        let dir = fresh_temp_dir("disk-norec");
        let hist = dir.join("foo.md.history");
        fs::create_dir(&hist).unwrap();
        fs::write(hist.join("a.md"), "x".repeat(10)).unwrap();
        // 创嵌套子目录 + 内部一个文件
        let inner = hist.join("nested");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join("b.md"), "x".repeat(999)).unwrap();

        let u = scan_history_disk_usage(&dir);
        // 只算顶层 a.md 的 10 字节，不递归进 nested
        assert_eq!(u.total_bytes, 10);
        assert_eq!(u.file_count, 1);
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
