//! Interactive list overlays for `/agents`, `/sessions`, `/tasks` and the
//! group's `/members` — same look and keys as the command palette (↑↓ move,
//! Enter confirms, Esc closes; Space toggles in multi-select). Selection acts
//! directly (switch agent/session, set members) instead of separate
//! `/agent <x>`-style commands.

use pet_core::session;
use pet_core::settings::{get_settings, AppSettings};
use pet_core::shell::TaskListItem;

/// What Enter does with the selected row.
pub enum PickerKind {
    SwitchAgent,
    SwitchSession,
    /// Set the active agent's model to the selected id.
    SwitchModel,
    /// Multi-select (Space toggles); Enter applies the checked set.
    Members,
    /// Informational list; Enter just closes.
    ViewOnly,
}

pub struct PickerItem {
    pub id: String,
    pub label: String,
    pub desc: String,
    /// `Some(bool)` in multi-select pickers.
    pub checked: Option<bool>,
}

pub struct Picker {
    pub title: String,
    pub kind: PickerKind,
    pub items: Vec<PickerItem>,
    pub sel: usize,
}

impl Picker {
    pub fn up(&mut self) {
        self.sel = self.sel.saturating_sub(1);
    }
    pub fn down(&mut self) {
        self.sel = (self.sel + 1).min(self.items.len().saturating_sub(1));
    }
    pub fn toggle(&mut self) {
        if let Some(item) = self.items.get_mut(self.sel) {
            if let Some(c) = item.checked {
                item.checked = Some(!c);
            }
        }
    }
    pub fn checked_ids(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| i.checked == Some(true))
            .map(|i| i.id.clone())
            .collect()
    }
}

/// `None` when there are no agents configured.
pub fn agents_picker() -> Option<Picker> {
    let settings = get_settings().ok()?;
    if settings.agents.is_empty() {
        return None;
    }
    let active = settings.active_agent_config().map(|a| a.id.clone()).unwrap_or_default();
    let sel = settings.agents.iter().position(|a| a.id == active).unwrap_or(0);
    let items = settings
        .agents
        .iter()
        .map(|a| PickerItem {
            id: a.id.clone(),
            label: a.name.clone(),
            desc: if a.id == active { format!("{} · 当前", a.model) } else { a.model.clone() },
            checked: None,
        })
        .collect();
    Some(Picker {
        title: " Agent（↑↓ 选择 · Enter 切换 · Esc 关闭）".to_string(),
        kind: PickerKind::SwitchAgent,
        items,
        sel,
    })
}

/// `None` when there are no sessions yet.
pub fn sessions_picker() -> Option<Picker> {
    let index = session::list_sessions();
    if index.sessions.is_empty() {
        return None;
    }
    let sel = index.sessions.iter().position(|m| m.id == index.active_id).unwrap_or(0);
    let items = index
        .sessions
        .iter()
        .map(|m| PickerItem {
            id: m.id.clone(),
            label: m.title.clone(),
            desc: if m.id == index.active_id {
                format!("{} · 当前", m.updated_at)
            } else {
                m.updated_at.clone()
            },
            checked: None,
        })
        .collect();
    Some(Picker {
        title: " 会话（↑↓ 选择 · Enter 切换 · Esc 关闭）".to_string(),
        kind: PickerKind::SwitchSession,
        items,
        sel,
    })
}

pub fn tasks_picker(tasks: &[TaskListItem]) -> Picker {
    let items = tasks
        .iter()
        .map(|t| PickerItem {
            id: t.task_id.clone(),
            label: format!("[{}] {}", t.kind, t.label),
            desc: format!("{} · {} ms", t.status, t.elapsed_ms),
            checked: None,
        })
        .collect();
    Picker {
        title: " 后台任务（Esc 关闭）".to_string(),
        kind: PickerKind::ViewOnly,
        items,
        sel: 0,
    }
}

/// Models fetched from the agent's OpenAI-compatible `/models` endpoint.
pub fn models_picker(current_model: &str, models: &[String]) -> Picker {
    let sel = models.iter().position(|m| m == current_model).unwrap_or(0);
    let items = models
        .iter()
        .map(|m| PickerItem {
            id: m.clone(),
            label: m.clone(),
            desc: if m == current_model { "当前".to_string() } else { String::new() },
            checked: None,
        })
        .collect();
    Picker {
        title: " 模型（↑↓ 选择 · Enter 切换 · Esc 关闭）".to_string(),
        kind: PickerKind::SwitchModel,
        items,
        sel,
    }
}

pub fn members_picker(settings: &AppSettings, members: &[String]) -> Picker {
    let items = settings
        .agents
        .iter()
        .map(|a| PickerItem {
            id: a.id.clone(),
            label: a.name.clone(),
            desc: a.model.clone(),
            checked: Some(members.contains(&a.id)),
        })
        .collect();
    Picker {
        title: " 群成员（Space 勾选 · Enter 应用 · Esc 取消）".to_string(),
        kind: PickerKind::Members,
        items,
        sel: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker(checked: &[bool]) -> Picker {
        Picker {
            title: String::new(),
            kind: PickerKind::Members,
            items: checked
                .iter()
                .enumerate()
                .map(|(i, &c)| PickerItem {
                    id: format!("a{i}"),
                    label: format!("agent{i}"),
                    desc: String::new(),
                    checked: Some(c),
                })
                .collect(),
            sel: 0,
        }
    }

    #[test]
    fn models_picker_marks_and_preselects_current_model() {
        let models = vec!["a-model".to_string(), "b-model".to_string(), "c-model".to_string()];
        let p = models_picker("b-model", &models);
        assert_eq!(p.sel, 1);
        assert_eq!(p.items[1].desc, "当前");
        assert!(p.items[0].desc.is_empty());
        // Unknown current model (e.g. hand-edited config) falls back to the top.
        assert_eq!(models_picker("gone", &models).sel, 0);
    }

    #[test]
    fn navigation_clamps_to_bounds() {
        let mut p = picker(&[false, false, false]);
        p.up();
        assert_eq!(p.sel, 0);
        p.down();
        p.down();
        p.down(); // past the end
        assert_eq!(p.sel, 2);
    }

    #[test]
    fn toggle_flips_only_the_selected_row_and_checked_ids_reflect_it() {
        let mut p = picker(&[true, false]);
        p.down();
        p.toggle();
        assert_eq!(p.checked_ids(), vec!["a0".to_string(), "a1".to_string()]);
        p.toggle();
        assert_eq!(p.checked_ids(), vec!["a0".to_string()]);
    }
}
