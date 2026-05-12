# PanelDebug "✏️ 临时 prompt fire" modal

## 需求

调 prompt 时常用流程："改 SOUL.md → 立即开口 → 看效果 → 改回 SOUL.md
原文 → 再去改另一种"。每次要走 PanelSettings 三步。补 PanelDebug 内
modal，让用户在 modal 内改 SOUL prompt + fire 一次（仅本轮生效，不
写盘）；revert 自动（FORCED_PROMPT_OVERRIDE take-once）。

## 实现

### 后端

`src-tauri/src/proactive/telemetry.rs`：

- 新 `pub static FORCED_PROMPT_OVERRIDE: Mutex<Option<String>>`，与
  FORCED_TASK_FOCUS 同 take-once 语义
- `reset_proactive_stash` 加清此 static

`src-tauri/src/proactive.rs`：

- `run_proactive_turn` 起跑时 take FORCED_PROMPT_OVERRIDE 替代 get_soul()
- 新 tauri command `trigger_proactive_turn_with_prompt(soul_override: String)`：
  - 校验 soul_override 非空
  - 写 FORCED_PROMPT_OVERRIDE + RAII ClearOnDrop defer-clear（panic 兜底）
  - 调既有 trigger_proactive_turn（path 同源，speech_count / feedback /
    decision_log 等正常 record）

`src-tauri/src/lib.rs`：注册 `trigger_proactive_turn_with_prompt`。

### 前端

`src/components/panel/PanelDebug.tsx`：

- 新 state `tempPromptOpen` / `tempPromptDraft` / `tempPromptBusy`
- `openTempPromptModal`：set open + invoke get_soul 预填 draft（用户从原
  prompt 基础上改更省事）
- 新按钮 "✏️ 临时 prompt"（accent 紫底）紧贴"立即开口"按钮
- modal：640 宽 / 80vh 高，单 textarea + 字符计数 + 取消 / 🚀 fire 按钮
  - busy 期间全 disable 防双触
  - fire 成功后 modal close + status toast 显结果 + 刷新 last manual fire
  - 失败时 toast 红字
  - backdrop / Esc / ✕ 关 modal

## 验证

- `cargo check`：clean
- `npx tsc --noEmit`：clean
- 行为：
  - 点 "✏️ 临时 prompt" → modal 弹开 + textarea 预填当前 SOUL.md
  - 改 prompt → 点 "🚀 fire 一次" → "开口中…"
  - 完成 → modal 关 + status toast 显 fire 结果 + audit 行更新
  - SOUL.md 文件未变（"临时"语义）
  - 紧接的下一轮自然 / 普通 fire → 用原 SOUL（take-once 防漏）
  - 取消 / Esc / backdrop → 不 fire 不写
  - 重置 stash 一并清 FORCED_PROMPT_OVERRIDE

## 不在本轮范围

- 没做"保留多个 prompt 变种"管理：单 modal 一次性；多变种是 prompt
  experiment 工具范畴，scope 翻倍
- 没做"diff vs SOUL.md" 视图：textarea 单段够用；diff highlight 需
  monaco 等
- 没做"保存到 SOUL.md" 按钮（modal 内直接持久）：与既有 PanelSettings
  SOUL 编辑器路径分离，避免 modal 承担两种语义
- 没在 modal 内显 fire 的 prompt + reply 全文：完成后 audit 行 + "看上
  次 prompt" modal 已经覆盖

## TODO 池剩余

空。下一轮需自主提需求。
