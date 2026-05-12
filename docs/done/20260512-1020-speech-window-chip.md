# PanelDebug 主动开口可调窗口

## 需求

PanelStatsCard 固定显"今日 X / 本周 Y / 累计 Z"，但用户想看"近 3 天 /
近 14 天 / 近 30 天"等中间窗口（如比较"上周 vs 这周"或观察"30 天活跃节
奏"）。现在没入口。加可调 chip 1d / 3d / 7d / 14d / 30d。

## 实现

### 后端 `src-tauri/src/speech_history.rs`

- 新 `get_speech_count_days(days: u32) -> u64` Tauri 命令：
  - days clamp 到 [1, 365] 防恶意 / 极端值
  - 复用既有 pure `sum_recent_days(map, today, n)` —— daily_path 读 JSON
    map + 求和
  - 与既有 `today_speech_count` / `week_speech_count` 共享同源（差只在窗
    口大小），单测覆盖在 sum_recent_days 那里已有
- 注册到 `lib.rs`

### 前端 `src/components/panel/PanelDebug.tsx`

- 既有 today / week state 保留（PanelStatsCard / markdown 导出仍用）
- 新 state：
  - `speechWindowDays: number`（初值 lazy 从 `localStorage["pet-debug-speech-window-days"]`
    读，限白名单 [1,3,7,14,30]，默认 3）
  - `speechWindowCount: number`
- 新 useEffect: `[speechWindowDays]` 变化触发：
  - 立即 invoke `get_speech_count_days`
  - 30s 轮询保活（与 debug snapshot 节奏对齐，节省 IPC）
  - cleanup 设 `cancelled` 防 race
- 在 PanelStatsCard 下方加 chip 行：
  - "近 [1d 3d 7d 14d 30d]"
  - 当前选中 chip accent 实底，其它 muted card 底
  - 右侧大字 monospace 显数字 + "次 · X.Y/日均"

## 验证

- `cargo check` clean
- `npx tsc --noEmit` clean
- 行为：
  - PanelDebug 进入 → 默认显"近 3d"窗口 + 数字 + 日均
  - 点 7d → 立刻 fetch + 数字更新 + localStorage 写
  - 切到别的 panel 再回 → 偏好保留
  - 30s polling 自动跟随 speech_daily.json 写入（每次主动开口 backend 都
    update 那 file）
  - 选 1d → 数字与既有 PanelStatsCard 的"今日"完全一致（同一源 + 同一窗口）

## 不在本轮范围

- 没把 PanelStatsCard 整体改可调：那卡有 4-5 个 fixed 数字，整体配色 +
  辅助派生（日均 / 克制阈值等）依赖 today/week，重写代价大；本轮加一行
  chip 已能补齐"任意窗口"需求
- 没做 LLM 日志的同款窗口选择（TODO 描述提到的"与 LLM 日志窗口语义对齐"）：
  LlmLogView 已有"加载更早 +N"模式；统一概念需要更深 UI 重构，留 follow-up
- 没加自定义 N 输入框：5 个预设覆盖常见 1d / 工作周 / 月维度，自由输入
  反而增加误操作

## TODO 池剩余

- PanelMemory consolidate 进度 + cancel
