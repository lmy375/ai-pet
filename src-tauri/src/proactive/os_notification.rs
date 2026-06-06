//! GOAL 052: OS notification channel — pet 不在前台时 proactive utterance
//! 同步走系统通知。

use tauri::AppHandle;

/// 前台状态：(is_foreground, since_when)。`since_when` 用 Instant（进程内
/// 单调时钟）—— 不需要绝对 wall-clock，纯量「失焦持续多久」。重启清
/// 零，重启后第一次 focus/blur 事件就重新填好。
///
/// None 表示从未收到 focus 事件——视作"unknown" 谨慎处理（默认前台，
/// 不发 notification）。
pub static WINDOW_FOREGROUND_STATE: std::sync::Mutex<Option<(bool, std::time::Instant)>> =
    std::sync::Mutex::new(None);

/// proactive utterance 类型 → notification group identifier 映射。
/// macOS 通知中心按 group 折叠（spec「便于 macOS 通知中心折叠」）。
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ProactiveNotificationKind {
    Briefing,
    Reminder,
    Surprise,
    Followup,
    Other,
}

impl ProactiveNotificationKind {
    pub fn group_id(self) -> &'static str {
        match self {
            Self::Briefing => "pet.briefing",
            Self::Reminder => "pet.reminder",
            Self::Surprise => "pet.surprise",
            Self::Followup => "pet.followup",
            Self::Other => "pet.other",
        }
    }
}

/// 不在前台至少多少秒后才走 OS 通道（spec 30s）。
pub const NOTIFICATION_FOREGROUND_THRESHOLD_SECS: u64 = 30;
/// notification body 最大 char 数（spec ≤ 60 字）。中文按 char 计安全。
pub const NOTIFICATION_BODY_CHAR_CAP: usize = 60;

/// 由 lib.rs window-event hook 调——更新前台状态。
pub fn update_window_focus(in_foreground: bool) {
    if let Ok(mut g) = WINDOW_FOREGROUND_STATE.lock() {
        let now = std::time::Instant::now();
        // 已是同状态不刷 since——保持原 since 计算"持续多久"。
        match *g {
            Some((prev, _)) if prev == in_foreground => {}
            _ => *g = Some((in_foreground, now)),
        }
    }
}

/// Pure：根据状态 + now 决定是否应该发 OS 通知。
/// - 前台 → false（user 看着 ChatMini，无需打扰）
/// - 状态未知（None）→ false（保守不发，避 startup 误触发）
/// - 后台 ≥ threshold_secs → true
pub fn should_send_os_notification(
    state: Option<(bool, std::time::Instant)>,
    now: std::time::Instant,
    threshold_secs: u64,
) -> bool {
    match state {
        Some((false, since)) => {
            now.saturating_duration_since(since).as_secs() >= threshold_secs
        }
        _ => false,
    }
}

/// Pure：截 utterance 首句作 notification body。spec「utterance 首句截断 ≤
/// 60 字」。"首句"识别中英常见句末标点：`. ! ? 。 ！ ？ \n`。无标点时整段
/// 截 cap。
pub fn format_notification_body(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // 找首句末标点（按 char 遍历避免 UTF-8 byte 切割）
    let sentence: String = {
        let mut acc = String::new();
        for c in trimmed.chars() {
            acc.push(c);
            if matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
                break;
            }
        }
        acc
    };
    let chars: Vec<char> = sentence.chars().collect();
    if chars.len() <= max_chars {
        sentence
    } else {
        let mut s: String = chars.into_iter().take(max_chars).collect();
        s.push('…');
        s
    }
}

/// 异步壳：调 Tauri notification plugin 发送一条 OS 通知。失败 silent
/// log——通知是「锦上添花」，绝不阻塞主 emit 路径。
///
/// caller 已用 [`should_send_os_notification`] gated；此函数不再二次检查
/// 前台状态，直接发。
pub async fn send_os_notification(
    app: &AppHandle,
    pet_name: &str,
    body: &str,
    kind: ProactiveNotificationKind,
) {
    use tauri_plugin_notification::NotificationExt;
    let title = if pet_name.trim().is_empty() {
        "Pet".to_string()
    } else {
        pet_name.trim().to_string()
    };
    let result = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .group(kind.group_id())
        .show();
    if let Err(e) = result {
        log::warn!(
            "send_os_notification({}): {}",
            kind.group_id(),
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_notification_body, should_send_os_notification, NOTIFICATION_BODY_CHAR_CAP,
        ProactiveNotificationKind,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn should_send_false_when_foreground() {
        let now = Instant::now();
        // 前台 60s（远超 threshold） — 仍不发
        let state = Some((true, now - Duration::from_secs(60)));
        assert!(!should_send_os_notification(state, now, 30));
    }

    #[test]
    fn should_send_false_when_state_unknown() {
        // 启动初 / 未收过 focus 事件 — 保守不发
        assert!(!should_send_os_notification(None, Instant::now(), 30));
    }

    #[test]
    fn should_send_false_when_background_under_threshold() {
        let now = Instant::now();
        // 后台 10s（< 30s threshold） — 仍不发
        let state = Some((false, now - Duration::from_secs(10)));
        assert!(!should_send_os_notification(state, now, 30));
    }

    #[test]
    fn should_send_true_when_background_over_threshold() {
        let now = Instant::now();
        // 后台 31s — 发
        let state = Some((false, now - Duration::from_secs(31)));
        assert!(should_send_os_notification(state, now, 30));
    }

    #[test]
    fn should_send_true_at_exact_threshold() {
        let now = Instant::now();
        // 后台 30s — 命中（≥ 边界含等号）
        let state = Some((false, now - Duration::from_secs(30)));
        assert!(should_send_os_notification(state, now, 30));
    }

    #[test]
    fn body_uses_first_sentence_when_present() {
        let s = format_notification_body("今天天气真好。我们去散步吧！", 60);
        // 首句末标点 `。` 后即截断
        assert_eq!(s, "今天天气真好。");
    }

    #[test]
    fn body_handles_english_punctuation() {
        let s = format_notification_body("Hello world! Second sentence.", 60);
        assert_eq!(s, "Hello world!");
    }

    #[test]
    fn body_truncates_long_first_sentence_with_ellipsis() {
        let long = "记".repeat(NOTIFICATION_BODY_CHAR_CAP + 10);
        let s = format_notification_body(&long, NOTIFICATION_BODY_CHAR_CAP);
        // 截 cap + …  →  cap + 1 chars
        assert!(s.ends_with('…'));
        assert_eq!(s.chars().count(), NOTIFICATION_BODY_CHAR_CAP + 1);
    }

    #[test]
    fn body_no_punctuation_truncates_to_cap() {
        // 无标点全段——截到 cap
        let s = format_notification_body("没有标点的一长串内容连续不断写", 5);
        assert!(s.ends_with('…'));
        assert_eq!(s.chars().count(), 6);
    }

    #[test]
    fn body_empty_input_returns_empty() {
        assert_eq!(format_notification_body("", 60), "");
        assert_eq!(format_notification_body("   ", 60), "");
    }

    #[test]
    fn body_first_line_break_treated_as_sentence_end() {
        let s = format_notification_body("第一行\n第二行更多内容", 60);
        // \n 视作首句末——截断在它之前 + \n 本身
        assert!(s.contains("第一行"));
        assert!(!s.contains("第二行"));
    }

    #[test]
    fn notification_kind_group_ids_distinct() {
        let groups = [
            ProactiveNotificationKind::Briefing.group_id(),
            ProactiveNotificationKind::Reminder.group_id(),
            ProactiveNotificationKind::Surprise.group_id(),
            ProactiveNotificationKind::Followup.group_id(),
            ProactiveNotificationKind::Other.group_id(),
        ];
        // 5 个 group id 必须两两不等——macOS 通知中心按 group 折叠
        for i in 0..groups.len() {
            for j in (i + 1)..groups.len() {
                assert_ne!(groups[i], groups[j], "group ids must be distinct");
            }
        }
        // 命名约定：均以 pet. 前缀（避免与其他 app group 冲突）
        for g in &groups {
            assert!(g.starts_with("pet."));
        }
    }
}
