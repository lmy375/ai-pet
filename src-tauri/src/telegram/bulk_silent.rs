//! Bulk-silence butler_tasks for a time window, with backend tokio timer
//! auto-release. Used by `/silent_all [minutes]` TG command. Desktop has its
//! own equivalent in `PanelMemory.tsx` (iter #366) using a frontend timer +
//! localStorage — the two surfaces don't share state by design (TG owner and
//! desktop owner may want independent windows).
//!
//! Behavior:
//! - `arm(titles, minutes)`：reads butler_task titles, applies `[silent]`
//!   marker per title via `task_set_silent`, stores snapshot in static
//!   `STORE`, and spawns a tokio task that sleeps `minutes` then auto-undoes.
//! - 第二次 arm 会先 `release_active` 旧窗口（释放原 titles），再 arm 新窗口。
//!   spawned timer 用 generation counter 判断"我是否还是 current"，过期 timer
//!   noop（avoid 重复 cleanup race）。
//! - `release_active()`：手动早解除 — 撤销 markers + 清 STORE。
//! - **不持久化**：app restart 会丢 timer，markers 留在原地（owner 可
//!   `/unsilent <title>` 单条清，或下次 `/silent_all` 让 arm 流程内置的
//!   release_active 处理）。trade-off：复用既有 `[silent]` marker schema
//!   零成本，代价是 restart 边界态依赖 owner 显式 cleanup。

use chrono::{DateTime, Duration as ChronoDuration, Local};
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct BulkSilentState {
    /// Titles 本窗口标 silent 的 — release 时按此 list 逐条 set_silent(false)。
    pub titles: Vec<String>,
    /// 到期自动 release 的本地时刻。
    pub expires_at: DateTime<Local>,
    /// 每次 arm 自增；spawned timer 用它判断"我是否还是 current 窗口"。
    pub generation: u64,
}

struct Store {
    inner: StdMutex<Option<BulkSilentState>>,
    counter: StdMutex<u64>,
}

static STORE: OnceLock<Store> = OnceLock::new();

fn store() -> &'static Store {
    STORE.get_or_init(|| Store {
        inner: StdMutex::new(None),
        counter: StdMutex::new(0),
    })
}

/// 当前 active 窗口 clone（None = 无 active）。formatter / `/silenced` 看
/// 这个判断 active 状态。
pub fn snapshot() -> Option<BulkSilentState> {
    store().inner.lock().ok().and_then(|g| g.clone())
}

/// 释放 active 窗口：撤销 markers + 清 STORE。返被撤销的 titles list
/// （caller 用于 reply count）。无 active → None。
pub fn release_active() -> Option<Vec<String>> {
    let snapshot = {
        let mut g = store().inner.lock().ok()?;
        g.take()
    }?;
    for title in &snapshot.titles {
        let _ = crate::commands::task::task_set_silent(title.clone(), false);
    }
    Some(snapshot.titles)
}

/// arm 新窗口：先 release_active（如果有），再对 `titles` 逐条
/// set_silent(true)，spawn timer auto-release after `minutes`。
///
/// 返新 state。空 titles / minutes <= 0 → Err 拒绝（caller 应在更高层
/// 早早 short-circuit usage hint）。
pub fn arm(titles: Vec<String>, minutes: i64) -> Result<BulkSilentState, String> {
    if titles.is_empty() {
        return Err("no titles to silence".to_string());
    }
    if minutes <= 0 {
        return Err("minutes must be positive".to_string());
    }

    // 释放 prior（如果有），保 markers 不堆积
    let _ = release_active();

    // 应用 marker；失败容忍（per-title set_silent 错误不阻塞其余）
    let mut applied: Vec<String> = Vec::new();
    for title in &titles {
        if crate::commands::task::task_set_silent(title.clone(), true).is_ok() {
            applied.push(title.clone());
        }
    }
    if applied.is_empty() {
        return Err("failed to silence any task".to_string());
    }

    let expires_at = Local::now() + ChronoDuration::minutes(minutes);
    let gen = {
        let mut counter = store()
            .counter
            .lock()
            .map_err(|e| format!("counter lock poisoned: {e}"))?;
        *counter = counter.wrapping_add(1);
        *counter
    };
    let state = BulkSilentState {
        titles: applied.clone(),
        expires_at,
        generation: gen,
    };
    {
        let mut g = store()
            .inner
            .lock()
            .map_err(|e| format!("store lock poisoned: {e}"))?;
        *g = Some(state.clone());
    }

    // Spawn timer。捕获 generation 检查"我是否过期" — 防 race（先后两次
    // arm，旧 timer 醒来时若新 arm 已生效，旧 timer noop 不动新 snapshot）。
    let captured_gen = gen;
    let captured_titles = applied.clone();
    let duration = std::time::Duration::from_secs(
        (minutes as u64).saturating_mul(60),
    );
    tokio::spawn(async move {
        tokio::time::sleep(duration).await;
        let still_current = match store().inner.lock() {
            Ok(g) => g
                .as_ref()
                .map(|s| s.generation == captured_gen)
                .unwrap_or(false),
            Err(_) => false,
        };
        if !still_current {
            return;
        }
        for title in &captured_titles {
            let _ = crate::commands::task::task_set_silent(title.clone(), false);
        }
        if let Ok(mut g) = store().inner.lock() {
            *g = None;
        }
    });

    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 重置 STORE — 因 OnceLock 不能 reset，用 inner lock 清 state 模拟。
    /// 测试间共享 STORE，每个测试自己负责 release。
    fn reset() {
        if let Ok(mut g) = store().inner.lock() {
            *g = None;
        }
    }

    #[test]
    fn snapshot_starts_none() {
        reset();
        assert!(snapshot().is_none());
    }

    #[test]
    fn release_active_returns_none_when_no_active() {
        reset();
        assert!(release_active().is_none());
    }

    // arm() 集成测试需 task_set_silent 后端就绪（含 SQLite kv + butler_tasks
    // 文件），不在 unit test 范围 — 留给手测 / e2e。本模块的 generation 计
    // 数器 / state 转移逻辑可单独验证：

    #[test]
    fn generation_increments_monotonically() {
        reset();
        let g1 = {
            let mut c = store().counter.lock().unwrap();
            *c = c.wrapping_add(1);
            *c
        };
        let g2 = {
            let mut c = store().counter.lock().unwrap();
            *c = c.wrapping_add(1);
            *c
        };
        assert!(g2 > g1, "generation should monotonically increase");
    }
}
