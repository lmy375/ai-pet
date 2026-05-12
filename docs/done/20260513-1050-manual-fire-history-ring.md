# PanelDebug manual fire 历史 ring (近 5 条)

## 需求

iter #187 给 PanelDebug 加了"上次 manual fire"audit 行（仅显最近一次）。
但调 prompt 或测试多次后想回看"今天点过哪些 fire"—— 单条记录不够。
扩展为 ring buffer (cap 5)。

## 实现

### 后端

`src-tauri/src/proactive/telemetry.rs`：

- 新 `pub static LAST_MANUAL_FIRE_HISTORY: Mutex<VecDeque<ManualFireRecord>>`
  + `pub const MANUAL_FIRE_HISTORY_CAP: usize = 5`
- 新 `push_manual_fire_history(record)` helper：尾插 + 超容 FIFO pop_front
- 新 tauri command `get_manual_fire_history() -> Vec<ManualFireRecord>`
  最新在前（reverse）
- 既有 `reset_proactive_stash` 增加清 history 一行
- 原 `LAST_MANUAL_FIRE` 单 record + 新 history ring 并行写：单 record
  给"最近一条"快路径，ring 给"近 N 条"列表

`src-tauri/src/proactive.rs`：

- `trigger_proactive_turn` 末段 build record 同时写 LAST_MANUAL_FIRE +
  push_manual_fire_history（共用 clone）
- `trigger_proactive_turn_for_task` wrapper 把 title 改写传递到两处：
  LAST_MANUAL_FIRE.title 与 LAST_MANUAL_FIRE_HISTORY.back_mut().title

`src-tauri/src/lib.rs`：注册 `proactive::get_manual_fire_history`。

### 前端

`src/components/panel/PanelDebug.tsx`：

- 新 state `manualFireHistory: ManualFireRecord[]` + `manualFireHistoryExpanded:
  boolean`
- `refreshLastManualFire` Promise.all 并发拉 last + history（老 backend
  无 history 命令时 catch → []，graceful）
- audit 行加 ▾/▸ N 展开按钮（仅 history > 1 时浮）
- 展开后下方 list 显 entries[1..]（跳过最新那条，避免重复）：
  - 每条 dashed-top 分隔，timestamp + 全局/per-task + result text
  - result 失败时红色
- 折叠 / 展开 toggle 状态不持久化（panel-local，session 内）

## 验证

- `cargo check`：clean
- `npx tsc --noEmit`：clean
- 行为：
  - 启动后无 fire → 整 audit 行不浮
  - 点 1 次立即开口 → audit 行浮单条，无 ▾ 按钮（history=1）
  - 点 4 次后 → audit 行显最新条 + "▸ 4" 按钮，点击展开下方 3 条 list
  - 点 6 次后 → ring FIFO 挤掉最早，仍显 5 条
  - 失败的 fire（"触发失败：..."）→ result 列红色，无论在最新条还
    是 history list 都一致
  - per-task fire 的 ▶️ 「title」标识在 history list 里也准确
  - 🔄 refresh + 重置 stash 都正确清两侧
  - 老 backend（无 get_manual_fire_history）→ 回退到只显单条，不报错

## 不在本轮范围

- 没让 ring buffer 持久化跨重启：与 LAST_PROACTIVE_TURNS 同语义（进
  程内 only），调 prompt 场景不需要历史持久
- 没做按 outcome 过滤（如仅显失败 fire）：5 条 cap 直接扫读够；要
  filter 等用户提需求
- 没加 audit 行 click-to-expand-modal（看完整 result text）：result
  hover 已 native title 显完整，modal 重
- 没集成"复制全部历史" markdown：当前没场景需求；future 可加

## TODO 池剩余

空。下一轮需自主提需求。
