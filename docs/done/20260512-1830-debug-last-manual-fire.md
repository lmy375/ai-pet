# PanelDebug "上次 manual fire" 行

## 需求

manual fire 有两个入口：PanelDebug 的 "立即开口" 与 PanelMemory 的
"▶️ 现在跑"。用户点完 → 看到一行短反馈就消失了，没地方回顾"我刚才
触发的是哪条 / 什么时候 / 结果"。给 PanelDebug 一行常驻 audit info。

## 实现

### 后端

`src-tauri/src/proactive/telemetry.rs`：

- 新 struct `ManualFireRecord { timestamp, title: Option<String>, result }`
  + `pub static LAST_MANUAL_FIRE: Mutex<Option<ManualFireRecord>>`
- 新 tauri command `get_last_manual_fire() -> Option<ManualFireRecord>`
- 进程内 only（与既有 LAST_PROACTIVE_* 同寿命）；自然 tick / 后台 loop
  不进此 stash —— 只 manual 入口记录

`src-tauri/src/proactive.rs`：

- `trigger_proactive_turn`：在尾段算完 user-facing response_string 后
  写入 LAST_MANUAL_FIRE（title=None 默认是全局 fire）。复用同一份
  response_string 让 "触发失败 / 开口完成 / 宠物选择沉默" 三种结局都
  被审计。result.map(|_| response_string) 保持原 Result 返回 shape
- `trigger_proactive_turn_for_task`：delegate 完后，把 LAST_MANUAL_FIRE
  的 title 字段从 None 改成 Some(本次目标)。两次写入串行 + UI 串行触
  发（firingProactive 全局 in-flight 标志），race 不会出现

`src-tauri/src/lib.rs`：注册 `proactive::get_last_manual_fire`。

### 前端

`src/components/panel/PanelDebug.tsx`：

- 新 state `lastManualFire: ManualFireRecord | null`
- `refreshLastManualFire()`：invoke get_last_manual_fire；失败静默忽略
- 挂载 useEffect 拉一次
- `handleTriggerProactive` 完毕 finally 块调一次 refresh —— 用户自己点
  立即开口后 audit 行 immediate 更新
- 渲染条件行（lastManualFire != null 才浮）：
  - 🕒 上次 manual fire
  - 等宽时间戳
  - "全局 fire" 或 `▶️ 「title」`（per-task）
  - result text + ellipsis（title attr 显完整）
  - 🔄 refresh 按钮（在另一个 chrome 窗口 fire 后回此 panel 同步）
- 失败结果（`触发失败：...` 开头）→ result 文本走 red 配色

## 验证

- `cargo check`：clean
- `npx tsc --noEmit`：clean
- 行为：
  - 进程冷启 → 没记录 → audit 行不浮
  - 点 PanelDebug "立即开口" → 完成后 audit 行浮 "🕒 上次 manual fire
    YYYY-MM-DD HH:MM:SS · 全局 fire | 开口完成 (X ms, idle=Ys): ..."
  - 切到 PanelMemory 点 "▶️ 现在跑" 某条 → 切回 PanelDebug 点 🔄 →
    audit 行 title 段变 "▶️ 「TITLE」"
  - 触发失败 → result 红色 + 文本 "触发失败：..."
  - 进程重启 → audit 行又消失（无持久化）

## 不在本轮范围

- 没做 ring buffer 显历史 N 条 fire：当前只显最近一次；多条 audit 需
  另设 stash + UI 滚动列表，工作量翻倍
- 没把 manual fire 写进 decision_log：那是 LLM 决策侧的事件流，与"用
  户手动 fire" 是不同 axis；混在一起会让 decision_log 噪声
- 没做 panel 离线时 backend 推 manual fire 完成事件：当前是 polling
  pull，足够低频场景。要 push 得加 emit("manual-fire-complete")
- 没让 audit 行点击 → 跳到对应 task / decision 日志：信息密度已经
  够；跳转需要 cross-panel 焦点逻辑

## TODO 池剩余

- PanelChat 双击 `「task title」` ref → 切到 PanelTasks tab + scroll 到该任务卡
