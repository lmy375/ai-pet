# 桌面气泡 👍 反馈按钮 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 桌面气泡 👍 反馈按钮：feedback_history 现仅有 replied / ignored / dismissed 三态，缺显式正向；气泡加 👍 一键写 Liked 信号，让 cadence 自适应有更高质量正反馈。

## 目标

`feedback_history.log` 当前三类信号：
- `replied` — 用户在两次 proactive 之间发了消息（语义松：可能是单字"嗯"也可能是高质量回应）
- `ignored` — 完全没互动（被动负反馈）
- `dismissed` — 5s 内点掉气泡（主动负反馈，R1b）

**缺一个主动正向**信号 —— 用户喜欢这条 proactive 但不想停下手头工作回复时，
没有低成本表达"赞"的方式。本轮加 `Liked` 第四态：气泡右上角加一个小 👍，点击
=「这条说得对/我喜欢」，写 `Liked` 进 feedback_history、立即消失气泡，**不**
冒泡到 `record_bubble_dismissed`。

## 非目标

- 不做 👎（负向）—— 主动负向已有 dismissed（5s 内点气泡），按钮过多反倒模糊语义。
- 不做"评论"输入 —— 想详细反馈用户会直接发消息，那就是天然的 `replied`。
- 不改 cadence 自适应公式（`adapted_cooldown_seconds` 等）—— 本轮先把信号种类加进
  来，让聚合 ratio 和 trailing-streak 都自动把 Liked 算成正向；公式调整等数据攒
  够再做。
- 不写 README —— 体验补强，与 R1b dismissed 同级别。

## 设计

### 后端

`feedback_history.rs`：

1. `FeedbackKind` 加 `Liked` variant；`as_str` 返回 `"liked"`。
2. `parse_line` 加 `"liked" => FeedbackKind::Liked` 分支。
3. **聚合 helpers 更新**（关键）：
   - `negative_signal_ratio`：当前过滤 `Ignored | Dismissed`。Liked 是正向，**不**
     入分子；分母仍包含（让"今天 5 条 proactive，3 条 liked / 1 条 ignored / 1 条
     dismissed"被算作 ratio = 2/5 = 0.4 而非纯负向占比）。
   - `count_trailing_negative`：Replied / Liked 都打断负向 streak（任一正向出现都
     重置）。
   - `classify_feedback_band`：把 Liked 与 Replied 同等对待（正向计数）。
4. 新 Tauri 命令 `record_bubble_liked(excerpt: String)`，与 `record_bubble_dismissed`
   完全对称：调 `record_event(FeedbackKind::Liked, &excerpt)`。
5. 注册到 lib.rs。

### 前端

`ChatBubble.tsx`：
- 右上角现已有 ✕（dismiss）。在 ✕ 左侧再加一个 👍 按钮，间距对称。
- 仅当 `onClick`（dismiss handler）已被传入时显示（与 ✕ 同 gating —— 都是
  dismissable proactive bubble 才有反馈意义）。
- 加 prop `onLike?: () => void`。点 👍 → `e.stopPropagation()` 防止冒泡触发
  主体 onClick（dismiss + 可能的 R1b 反馈）+ 调 `onLike`。
- 视觉：与 ✕ 同灰阶（`opacity 0.55`，hover 时变绿/橙提示交互可达）。

`App.tsx`：
- 加 `handleBubbleLike` —— 与 `handleBubbleClick` 平行：
  - `setBubbleDismissed(true)` 让气泡消失
  - 若有 displayMessage 且非历史模式，invoke `record_bubble_liked`
  - **不**调 `record_bubble_dismissed`（避免同时记两条相反信号）
- 把 `handleBubbleLike` 传进 ChatBubble 的新 `onLike` prop。
- 历史模式（`bubbleHistory.isHistoryMode`）下 👍 按钮**不渲染**——历史快照不应再
  接受新反馈（与 dismiss 不记 R1b 同语义）。需要在 ChatBubble 接受 `onLike?:
  undefined` 时跳过渲染。

### 测试

- `format_line` / `parse_line` round-trip Liked
- `negative_signal_ratio` 把 Liked 排除分子
- `count_trailing_negative` Liked 重置 streak
- `classify_feedback_band` Liked 计入正向

前端无测试基础设施，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `FeedbackKind::Liked` + format/parse + 聚合 helpers 更新 + 单测 |
| **M2** | `record_bubble_liked` Tauri 命令 + 注册 lib.rs |
| **M3** | ChatBubble onLike prop + 👍 渲染 |
| **M4** | App.tsx handleBubbleLike + 历史模式跳过 |
| **M5** | `cargo test` + `pnpm build` + TODO 清理 + done/ |

## 复用清单

- `feedback_history::record_event`（已有）
- `record_bubble_dismissed` 命令的 IO 模板（直接 mirror 一份）
- ChatBubble 现有 ✕ 按钮的视觉位置 / animation / stopPropagation 模式

## 待用户裁定的开放问题

- 👍 是否需要短暂的"已收到"反馈动画（如 +1 心动效果）？本轮选**不做**——气泡
  立即消失就是足够的反馈，加额外动画反而拖慢交互节奏。
- Liked 在 `negative_signal_ratio` 是否完全中立？本轮选**不入分子但入分母**——
  让 ratio 反映的是"在所有反馈中负向占比"，正向多了 ratio 自然下降。

## 进度日志

- 2026-05-05 08:00 — 创建本文档；准备 M1。
- 2026-05-05 08:30 — 完成实现：
  - **M1**：`feedback_history.rs` 加 `FeedbackKind::Liked` variant + `as_str` / `parse_line` 对应 `"liked"` 分支。`negative_signal_ratio` / `count_trailing_negative` 现有 `matches!(Ignored | Dismissed)` 自动让 Liked 不入分子且能 break streak。`format_feedback_aggregate_hint` 重构为 dynamic parts vec，liked / dismissed 两个段都按 > 0 条件展示。`format_feedback_hint` 加 Liked 分支，文案语义"延续这种语气"（与负向"调整"对比）。新增 6 条单测覆盖 round-trip / 排除分子 / streak 重置 / aggregate 显隐 / 文案语义。
  - **M2**：`record_bubble_liked(excerpt)` Tauri 命令（与 `record_bubble_dismissed` 对偶），注册到 lib.rs。`proactive.rs` 的 FeedbackSummary 构造 把 Liked 与 Replied 一起计入 `replied` 字段（保持 wire format 不变；panel chip 健康度语义"被听到"二者等价）。
  - **M3**：`ChatBubble.tsx` 加可选 `onLike?: () => void` prop。✕ 与 👍 重组到一个 flex 容器（右起 ✕ → 👍，gap 4px）。👍 用 button + `pet-bubble-like-btn` CSS class（hover 变粉 + 1.15x 放大），`stopPropagation` 防止冒泡触发 dismiss 路径。
  - **M4**：`App.tsx` 加 `handleBubbleLike`：`setBubbleDismissed(true)` + invoke `record_bubble_liked`，**不**调 dismissed（避免双写正负反馈）。`onLike` prop 仅在 `!isHistoryMode && !isLoading` 时传入（其他场景 undefined → 按钮不渲染）。`PanelToneStrip.tsx` chip tooltip 文案补"含回复 + 主动点赞 👍"。`PanelDebug.tsx` 反馈时间线加 liked filter 选项（粉色徽章）+ pill render 加 liked 分支。
  - **M5**：`cargo test --lib` 859/859（+6）；`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 反馈系统的内嵌补强（现有 R1b dismissed 的对偶），不是独立亮点。
  - **设计取舍**：Liked 是 Replied 的"高质量版本"——二者 ratio 计算等价（都算正向），但 aggregate 文案分开计数让 LLM 感知差异；不做 record_bubble_liked 的 awaiting 清除（主动点赞不必妨碍 auto-classify），双信号同时进 log 是事实记录，不是 bug。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数链路有单测钉牢，前端按钮交互由 tsc + 既有 ChatBubble 模式保证。
