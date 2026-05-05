//! TG bot 启动期非 fatal 失败的内存归档。冷启动 / 重连阶段的"自动补全
//! 注册失败 / bot 起不来" 这类信息原本只 eprintln 到 dev 控制台，用户
//! 看不到。本模块把它们集中到一个进程内 Vec，PanelDebug 拉出来展示。
//!
//! 进程重启清空（`std::sync::Mutex<Vec<...>>` in `Arc`）。不持久化到磁
//! 盘 —— 启动告警是当次进程的语境，重启后旧告警与现状无关，留存反而
//! 误导。
//!
//! 不替代 `commands::telegram::TelegramStatus.error`：那是 reconnect
//! 命令的同步反馈通道，本模块管的是冷启动 / 后续异步阶段（如
//! `set_my_commands`）的事后归档。两者互补。
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
pub struct TgStartupWarning {
    /// 失败时刻 RFC3339（chrono::Local）。
    pub timestamp: String,
    /// 来源标记：`bot_start` / `set_my_commands` / 其它（短 ascii
    /// 字符串便于前端 grep / 着色）。
    pub kind: String,
    /// 原始 error 串（teloxide / settings / 网络 etc 的 to_string）。
    pub message: String,
}

pub type TgStartupWarningStore = Arc<Mutex<Vec<TgStartupWarning>>>;

pub fn new_store() -> TgStartupWarningStore {
    Arc::new(Mutex::new(Vec::new()))
}

/// 追加一条告警。锁竞争为零（push/snapshot 都是纳秒级）；poison 时退化为
/// 静默丢弃 —— 启动告警本身是 best-effort 通道，不该把 lock poison 进一
/// 步上抛影响主流程。
///
/// **去重**：同 (kind, message) 已存在 → 只更新该条 timestamp（保留最新
/// 一次发生时刻），不追加新条目。这避免反复失败（如 set_my_commands 在
/// 网络长时间不通时每次 reconnect 都失败）把 store 撑爆 + 用户 banner
/// 上看到 N 条相同条目刷屏。
pub fn push(store: &TgStartupWarningStore, kind: &str, message: String) {
    let Ok(mut guard) = store.lock() else {
        return;
    };
    let timestamp = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
        .to_string();
    if let Some(existing) = guard
        .iter_mut()
        .find(|w| w.kind == kind && w.message == message)
    {
        existing.timestamp = timestamp;
        return;
    }
    guard.push(TgStartupWarning {
        timestamp,
        kind: kind.to_string(),
        message,
    });
}

/// 拿当前所有告警的快照（clone 出去）。
pub fn snapshot(store: &TgStartupWarningStore) -> Vec<TgStartupWarning> {
    store.lock().map(|g| g.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_snapshot_round_trips() {
        let s = new_store();
        push(&s, "bot_start", "auth failed".to_string());
        push(&s, "set_my_commands", "network timeout".to_string());
        let snap = snapshot(&s);
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].kind, "bot_start");
        assert_eq!(snap[0].message, "auth failed");
        assert_eq!(snap[1].kind, "set_my_commands");
        assert!(!snap[0].timestamp.is_empty());
    }

    #[test]
    fn snapshot_clones_so_caller_changes_dont_leak() {
        let s = new_store();
        push(&s, "k", "m".to_string());
        let mut snap = snapshot(&s);
        snap.clear();
        // 调用方 clear 不该影响内部
        assert_eq!(snapshot(&s).len(), 1);
    }

    #[test]
    fn empty_store_returns_empty_snapshot() {
        let s = new_store();
        assert!(snapshot(&s).is_empty());
    }

    #[test]
    fn push_dedupes_same_kind_and_message() {
        // 反复失败 → 不该让 store 无限堆；只保留 1 条 + 最新 ts
        let s = new_store();
        push(&s, "set_my_commands", "network timeout".to_string());
        let first_ts = snapshot(&s)[0].timestamp.clone();
        // 故意 sleep 1ms 让 ts 字串不同，验证更新发生
        std::thread::sleep(std::time::Duration::from_millis(2));
        push(&s, "set_my_commands", "network timeout".to_string());
        push(&s, "set_my_commands", "network timeout".to_string());
        let snap = snapshot(&s);
        assert_eq!(snap.len(), 1, "duplicates should not stack: {:?}", snap);
        assert_ne!(
            snap[0].timestamp, first_ts,
            "ts should be refreshed to most recent"
        );
    }

    #[test]
    fn push_distinguishes_different_messages() {
        // 同 kind 但 message 不同 → 视作两条独立告警
        let s = new_store();
        push(&s, "set_my_commands", "network timeout".to_string());
        push(&s, "set_my_commands", "auth invalid".to_string());
        assert_eq!(snapshot(&s).len(), 2);
    }

    #[test]
    fn push_distinguishes_different_kinds() {
        // message 相同但 kind 不同 → 两条
        let s = new_store();
        push(&s, "bot_start", "io error".to_string());
        push(&s, "set_my_commands", "io error".to_string());
        assert_eq!(snapshot(&s).len(), 2);
    }
}
