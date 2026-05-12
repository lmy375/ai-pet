# 设置页 motion_mapping 试播按钮

## 需求

motion_mapping 把 4 个语义键（Tap / Flick / Flick3 / Idle）映射到具体 model 的
motion group 名。用户改完后只有等下次主动开口 / 聊一句才知道"我填的 group 名
对不对"。在每行旁加"▶ 试一下"按钮，立刻在桌面 Live2D 上播一次目标 motion。

## 设计取舍

- **后端发 chat-done 事件，不引新 channel**：useMoodAnimation 已经监听 chat-done
  + proactive-message 两个事件并通过 triggerMotion 跑完整"映射翻译 + 容错"
  路径。复用这个 shape 等于免费拿到所有现有保护（边界 group 名错 throw 时
  console.debug 吃掉、priority=2 不打断高优）。
- **测试的是已保存的 mapping，不是输入框中的草稿**：后端从 settings 读 mapping
  字段，前端 motion_mapping 改了不保存就调试 → 还是测旧映射。tooltip 提醒"先
  保存"。让用户主动决定何时验，与 image / chat 测试按钮的语义对齐。

## 实现

### 后端

`src-tauri/src/commands/settings.rs`：

```rust
#[tauri::command]
pub fn trigger_motion(app: tauri::AppHandle, semantic: String) -> Result<(), String> {
    use tauri::Emitter;
    let payload = serde_json::json!({
        "motion": semantic,
        "mood": null,
        "timestamp": chrono::Local::now().to_rfc3339(),
    });
    app.emit("chat-done", payload)
        .map_err(|e| format!("emit motion test failed: {e}"))
}
```

emit 全 webview window — desktop pet 的 useMoodAnimation listener 会接到。

`src-tauri/src/lib.rs` 注册 `commands::settings::trigger_motion`。

### 前端

`src/components/panel/PanelSettings.tsx`：每行 motion mapping 的输入框后加"▶
试一下"按钮（accent 色），点击 invoke trigger_motion(semantic=key)。tooltip
强调"先保存才能验最新映射"。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 改完 mapping → 保存 → 点 ▶ 试一下 → 桌面 Live2D 立刻播对应 group 一次
  - group 名错（model 不存在该 group）→ console.debug 吃掉，不崩；用户看到桌面没动 → 知道映射写错
  - 同时打开桌面 + 设置 panel：可以反复点同一行 ▶ 验稳定性
  - 4 个 key 各有独立按钮，互不干扰

## TODO 池清空 → 自主提案

按规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. ChatMini 顶部 📋 弹框加"复制带时间"开关
2. PanelTasks 任务卡片拖拽调 priority
3. 设置页加 dark / light theme toggle 控件
4. /image -n 局部成功失败的混合反馈
5. ChatMini 角色 glyph 可配置（替 🧑/🐾）
