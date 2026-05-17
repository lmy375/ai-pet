//! 今日 mute proactive 计数：owner 临时按下「⚙️ mute N min」绕过 proactive
//! cycle 的次数，本地按日聚合。让 ChatMini 顶部「🔕 今日 mute」chip 显
//! owner audit "今天打断宠物几次"。
//!
//! 设计：
//! - 仅 in-process 内存计数（进程重启清零）。owner 实际诉求是当日感知，
//!   持久化到磁盘 / SQLite 增加 IO + race 不值得。
//! - 跨午夜自动 reset：date 字段对比当前日期不同 → 计数清零再 +1。读取时
//!   也校验 date — 跨午夜读返 0 避免显昨日 stale 数据。
//! - 只数 mute engage（minutes > 0），不数 mute clear（minutes <= 0）。
//!   后者是"解除静默"而非"启用静默"，与 chip "今天我让宠物闭嘴几次" 的
//!   语义相反。

use std::sync::Mutex;

/// 当日 mute engaged 计数：`date` 是 YYYY-MM-DD 本地日期 key，`count` 是
/// 本日累计 engage 次数。初始化为空 / 0；首次 `record_mute_engaged` 时填
/// 入当日 key + 1。
pub struct DailyMuteCount {
    pub date: String,
    pub count: u64,
}

pub static TODAY_MUTE: Mutex<DailyMuteCount> = Mutex::new(DailyMuteCount {
    date: String::new(),
    count: 0,
});

fn today_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 记录一次 mute engage（owner 按下 "⚙️ mute N min" 或 TG `/sleep N` 等
/// 路径）。跨午夜时先清零再 +1。
pub fn record_mute_engaged() {
    let key = today_key();
    if let Ok(mut g) = TODAY_MUTE.lock() {
        if g.date != key {
            g.date = key;
            g.count = 0;
        }
        g.count += 1;
    }
}

/// pure：从 `(date, count)` + 当前 today_key 算实际显示值。跨日返 0 让
/// 跨午夜读不显 stale。给单测用 — 生产路径走 `get_today_mute_count`。
pub fn today_count_from(state_date: &str, state_count: u64, today: &str) -> u64 {
    if state_date != today {
        0
    } else {
        state_count
    }
}

/// 今日 mute engaged 总数。跨午夜自动 reset（read 路径用 today_key 校验
/// state 的 date；不匹配 → 返 0 而非读 stale）。前端 chip 用。
#[tauri::command]
pub fn get_today_mute_count() -> u64 {
    let key = today_key();
    if let Ok(g) = TODAY_MUTE.lock() {
        return today_count_from(&g.date, g.count, &key);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_count_returns_zero_when_date_mismatches() {
        assert_eq!(today_count_from("2026-05-16", 5, "2026-05-17"), 0);
        assert_eq!(today_count_from("", 0, "2026-05-17"), 0);
    }

    #[test]
    fn today_count_returns_stored_when_date_matches() {
        assert_eq!(today_count_from("2026-05-17", 3, "2026-05-17"), 3);
        assert_eq!(today_count_from("2026-05-17", 0, "2026-05-17"), 0);
    }

    #[test]
    fn record_then_get_increments_today_bucket() {
        // 直接跟 TODAY_MUTE 互动 —— 注意此测试与其它测试共享静态，避免
        // 在并发测试 runner 里互相干扰：在测试开始时先归一到当日 key + 0。
        let today = today_key();
        {
            let mut g = TODAY_MUTE.lock().unwrap();
            g.date = today.clone();
            g.count = 0;
        }
        record_mute_engaged();
        record_mute_engaged();
        let n = get_today_mute_count();
        assert!(n >= 2, "expected at least 2 increments, got {}", n);
    }
}
