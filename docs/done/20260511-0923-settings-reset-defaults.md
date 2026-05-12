# 设置页"重置默认"按钮

## 需求

设置页改坏了想清重来 —— 当前要么手删 ~/.config/pet/config.yaml 要么逐字段
改回。给一个一键重置按钮，且二次确认防误触。

## 范围

只清"设置项"（config.yaml），不动：
- SOUL.md（用户写的角色设定）
- memory（LLM 长期记忆）
- sessions（聊天会话历史）
- butler_history（任务执行审计）
- current_mood.txt / morning_briefing_last.txt 等持久 state

理由：用户"重置设置"的预期是"配置面板回到出厂"，不是"删一切重来"。后者
危险太大，留给用户主动 rm。

## 实现

### 后端

`src-tauri/src/commands/settings.rs`：

```rust
#[tauri::command]
pub fn reset_config_to_defaults() -> Result<(), String> {
    let defaults = AppSettings::default();
    let yaml = serde_yaml::to_string(&defaults)?;
    fs::write(&config_path()?, yaml)?;
    Ok(())
}
```

`src-tauri/src/lib.rs` 注册命令。

### 前端

`src/components/panel/PanelSettings.tsx`：

- 新 state `resetArmed: boolean` + `resetArmTimerRef`
- `handleResetDefaults`：armed 模式（与 /clear 同一份 5s armed-state 模板）
  - 未 armed → setResetArmed(true) + setTimeout 5s 自动撤回 + setMessage 提示
  - armed → 清 timer + invoke reset_config_to_defaults + reload settings (get_settings + get_config_raw)
- Save 按钮旁加一个二级"重置默认"按钮
  - armed 态变红填充 + "⚠ 确认重置？"文案；非 armed 态走 muted 浅灰，避免与
    Save 主按钮抢眼
  - tooltip 强调"只清设置；SOUL / memory / sessions / butler_history 都不动"

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 点"重置默认" → 红 armed + 顶 message 提示"再点一次确认（5 秒内）" + 列出
    不会动的目录
  - 5s 内再点 → reset_config_to_defaults → reload settings → 表单字段全部回默认
  - 5s 后不点 → armed 自动撤回，按钮回灰态
  - SOUL 文本框 / memory 页 / sessions 数据不受影响
  - 重置失败（IO error）→ 红色 message

## 不在本轮范围

- 没做"导出当前设置为快照 + 一键恢复"：用户级别的 backup/restore 是更大功能；
  本轮先把"撤销误改"的最小路径打通。需要时手动备份 config.yaml 也能用
- 没做"重置后重启宠物窗口"：与 Save 同样的"保存成功！重启宠物窗口后生效"
  message，让用户自己决定是否重启
