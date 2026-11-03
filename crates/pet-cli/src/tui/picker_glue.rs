//! Picker overlay glue: acting on a selection and routing list commands.

use pet_core::session;

use crate::Mode;
use super::entries::Entry;
use super::picker::{self, PickerKind};
use super::Action;

impl super::TuiApp {
    /// Enter on a picker row: act on the selection.
    pub(super) fn confirm_picker(&mut self) -> Vec<Action> {
        let Some(p) = self.picker.take() else { return vec![] };
        match p.kind {
            PickerKind::SwitchAgent => {
                if let Some(item) = p.items.get(p.sel) {
                    match pet_core::settings::set_active_agent(&item.id) {
                        Ok(()) => self.push(Entry::Notice {
                            text: format!("已切换到 {}", item.label),
                        }),
                        Err(e) => self.push(Entry::Error { text: e }),
                    }
                    self.refresh_header();
                }
            }
            PickerKind::SwitchSession => {
                if let Some(item) = p.items.get(p.sel) {
                    match session::set_active_session(item.id.clone()) {
                        Ok(()) => {
                            // The turn we were watching (if any) runs on in its
                            // own session; this view now follows the new one and
                            // attaches to its turn, if it has one.
                            self.finish_streaming_assistant();
                            self.prune_empty_assistant();
                            self.busy = false;
                            self.push(Entry::Notice {
                                text: format!("已切换到会话：{}", item.label),
                            });
                            self.replay_session(8);
                            self.refresh_header();
                            return vec![Action::SyncTurn];
                        }
                        Err(e) => self.push(Entry::Error { text: e }),
                    }
                    self.refresh_header();
                }
            }
            PickerKind::SwitchModel => {
                if let Some(item) = p.items.get(p.sel) {
                    let agent_id = pet_core::settings::snapshot()
                        .ok()
                        .and_then(|s| s.active_agent_config().map(|a| a.id.clone()));
                    match agent_id {
                        Some(id) => match pet_core::settings::set_agent_model(&id, &item.id) {
                            Ok(()) => self.push(Entry::Notice {
                                text: format!("模型已切换为 {}", item.label),
                            }),
                            Err(e) => self.push(Entry::Error { text: e }),
                        },
                        None => self.push(Entry::Error { text: "没有可用的 Agent".to_string() }),
                    }
                    self.refresh_header();
                }
            }
            PickerKind::Members => {
                self.busy = true;
                return vec![Action::SetMembers(p.checked_ids())];
            }
            PickerKind::ViewOnly => {}
        }
        vec![]
    }

    /// List commands open their picker overlay instead of dispatching (and
    /// instead of dumping output into the transcript).
    pub(super) fn open_picker_for(&mut self, line: &str) -> Option<Vec<Action>> {
        match (self.mode, line) {
            (Mode::Chat, "/agents" | "/agent") => {
                match picker::agents_picker() {
                    Some(p) => self.picker = Some(p),
                    None => self.push(Entry::Notice { text: "还没有配置 Agent".to_string() }),
                }
                Some(vec![])
            }
            (Mode::Chat, "/sessions" | "/session") => {
                match picker::sessions_picker() {
                    Some(p) => self.picker = Some(p),
                    None => self.push(Entry::Notice {
                        text: "还没有会话（发送一条消息即可开始）".to_string(),
                    }),
                }
                Some(vec![])
            }
            (Mode::Chat, "/tasks") => Some(vec![Action::OpenTasks]),
            (Mode::Chat, "/models" | "/model") => Some(vec![Action::OpenModels]),
            (Mode::Group, "/members") => Some(vec![Action::OpenMembers]),
            _ => None,
        }
    }
}