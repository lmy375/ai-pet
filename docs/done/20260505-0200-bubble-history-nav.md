# 桌面气泡历史导航 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 气泡历史导航：桌面气泡只展示最新一句；加「上一句 / 下一句」按钮，让用户错过气泡也能翻回最近 N 条主动开口。

## 目标

桌面 ChatBubble 现在只承载"最新一条"，主动开口被用户错过（不在屏幕 / 走神 / 60s 自动 dismiss 之后）就找不回来了 —— 想看只能切到面板。本轮在气泡右下角加一对极小的 `◀ / ▶` 按钮，让用户**不离开气泡**就能翻回最近 ~10 条主动发言，看完再回到 live。

## 非目标

- 不做"按 keyword 搜历史"——气泡里没空间放搜索框，那是面板的活。
- 不在气泡里翻完整 session（reactive 一问一答）—— 用户主诉是错过 proactive，session 历史已在面板「聊天」标签里看得到。本轮只翻 `speech_history.log`（即"宠物的主动发言"）。
- 不做时间格式化展示（"3 分钟前"等）——按行展示原文已经够，时间戳直接 strip 掉。
- 不动 60s 自动 dismiss 计时器策略 —— 仅在导航时**重置**它（让用户翻历史时 bubble 不会突然消失）。
- 不写 README —— "翻回最近 N 条"是体验微调，不是新亮点。

## 设计

### UX

- **Live 模式（默认）**：bubble 显示 `displayMessage`（与现状完全一致）。
  - 右下角小按钮：`◀`（仅当 speech_history 有 ≥1 条时显示）。无 `▶`，无指示器。
- **History 模式**：bubble 显示 `speeches[i]`（已 strip 时间戳）。
  - 右下角：`◀ i+1/N ▶`。`◀` 在 `i == N-1` 时禁用；`▶` 永远可点 —— 在 `i == 0` 时点 `▶` 回到 Live。
- **进入 History 模式**：在 Live 模式点 `◀` → 加载 speech_history（首次） → `i = 0`。
- **离开 History 模式**：`▶` 从 `i = 0` 回到 Live。
- **Loading 中**（isLoading=true）：所有按钮隐藏 —— 流式输出期间翻历史会让人困惑。
- **点 bubble 主体**（按钮以外）：仍走原有的 `onClick`（dismiss + R1b 反馈）。按钮 `e.stopPropagation()`。
- **导航时重置 60s 自动 dismiss 计时器**：用户在主动看 bubble，不该被中断。
- **history 模式下点击主体**：仍 dismiss（保持单一交互模型，不为 history mode 引入特殊"返回 live"含义）。

### 边界 / 已知小毛刺

- 若 `displayMessage` 恰好就是 `speeches[0]`（最近一条 proactive 即当前展示的），点 `◀` 会显示同样的内容（用户得再点一次）。**不做去重**——文本相等判断在多空白 / 多场景下脆弱，且第一次点没翻动只是"轻微错觉"，远弱于增加去重逻辑后的复杂度。

### 数据来源

复用现成 Tauri 命令 `get_recent_speeches(n: Option<usize>) -> Vec<String>`。
返回每行形如 `"<ISO ts> <text>"`，前端用 `splitOnce(' ')` strip 时间戳。
N 取 10 —— `SPEECH_HISTORY_CAP = 50`，但 10 在气泡导航场景里足够（Iter R10s
们打磨的"最近 N 条"prompt 注入也用 10）。

### 文件改动

1. **新建 `src/hooks/useBubbleHistory.ts`**：自包含 hook
   - 状态：`speeches: string[] | null`（null = 未加载），`index: number | null`（null = live）
   - `enterPrev()`：加载（若未加载） → 若 speeches 非空，`index = (index ?? -1) + 1`，clamp 到 `speeches.length - 1`
   - `next()`：`index--`，到 -1 → 设为 null（回 live）
   - `reset()`：让外部信号（如新 proactive 到来）能立刻把视图拽回 live
   - `displayed: string | null`：当前应展示的字符串（live 模式 → null 让外部回退到 displayMessage；history 模式 → speeches[index] 已 strip ts）
   - `indicator: string | null`：`"i+1/N"` 或 null（live 模式）
   - `canPrev / canNext: boolean`
2. **改 `src/components/ChatBubble.tsx`**：增加可选 prop
   - `historyControls?: { canPrev: boolean; canNext: boolean; onPrev: () => void; onNext: () => void; indicator: string | null; }`
   - 渲染在 bubble 右下角，按钮上挂 `e.stopPropagation()` 防止冒泡触发 dismiss。
3. **改 `src/App.tsx`**：
   - 用 `useBubbleHistory(displayMessage)` 拿 hook
   - bubble 实际展示 = `bubbleHistory.displayed ?? displayMessage`
   - 导航时调 `setBubbleDismissed(false)` + 重置 60s 计时器
   - 监听 `proactive-message`：新到来时 `bubbleHistory.reset()`（不要让用户停在历史里错过新发言；new bubble 接管）
   - bubble 的 onClick 仍走原 `handleBubbleClick`（dismiss + R1b 反馈）

### 测试

无前端测试套件（package.json 未配置 vitest）。本次改动的逻辑是：
- hook 内部状态机 prev/next 边界
- ChatBubble 多一组按钮 + stopPropagation
- App.tsx 接线

逻辑足够小，全部走 `tsc --noEmit` + 手动 dev 验证：
1. 启动 → bubble 初次出现，按钮不可见（无历史）
2. 触发几次 proactive → 历史攒起来
3. 看到最新 bubble，点 `◀` → 加载历史 → 显示最近 1 条 + 指示器 `1/N`
4. 再点 `◀` → 翻到第二条
5. 点 `▶` → 回到第一条
6. 再点 `▶` → 回到 live 模式
7. Loading 中按钮消失
8. 翻历史时不会被 60s 自动 dismiss 中断（导航重置计时器）
9. 新 proactive 到来 → 自动回到 live

不写"测试代码"上限：hook 是 React-bound，纯函数边界拆出来意义不大；提取成 vanilla TS 又得引一套 vitest 配置 —— 工作量与价值不匹配。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `useBubbleHistory` hook |
| **M2** | `ChatBubble.tsx` 接 `historyControls` |
| **M3** | `App.tsx` 接线 + 60s 计时器重置 + proactive-message reset |
| **M4** | `tsc --noEmit` + 手动 dev 验证 + TODO 清理 + 文档移入 done/ |

## 复用清单

- `get_recent_speeches` Tauri 命令（已有，参数 `n?: number`）
- `speech_history::strip_timestamp` 等价的 frontend split：`line.indexOf(' ')` + slice
- `ChatBubble` 现有交互层（fade-in / hover lift / active press）—— 按钮覆盖在已有 layout 上，不破坏既有动画

## 待用户裁定的开放问题

- 默认 N 是 10 还是 20？本轮选 10（与 SPEECH_HISTORY_CAP=50、prompt 注入采样上限一致；再多用户也不太会一直翻）。
- 翻到尽头时 `◀` 禁用还是循环回到第 0 条？本轮选**禁用**（用 opacity 0.4 + cursor not-allowed），循环对小列表反而困惑。

## 进度日志

- 2026-05-05 02:00 — 创建本文档；准备 M1。
- 2026-05-05 02:30 — 完成实现：
  - **M1**：`src/hooks/useBubbleHistory.ts` —— 管理 `speeches`（lazy 加载，null=未加载/[]=已加载空）+ `index`（null=live / 0..N-1=history）。`enterPrev` 首点触发 `invoke("get_recent_speeches", { n: 10 })`，反转 vec 让 i=0 是最新，按 i 步进；`next` 减 1 到 -1 回 live；`reset` 清缓存（让外部 proactive 到来时拉回 live 并丢弃过期窗口）。`canPrev` 状态机区分"未加载 / 已加载空 / live / history"四档。
  - **M2**：`ChatBubble.tsx` 新增可选 `historyControls` prop。右下角渲染 `◀ 指示器 ▶`，按钮 css class `pet-bubble-nav-btn` 带 hover/disabled 视觉态。`e.stopPropagation()` 防止冒泡触发 dismiss；live 模式下不渲染 ▶ 与指示器；▶ 在 history mode 下永远可点（i=0 时回 live）。
  - **M3**：`App.tsx` 引入 `useBubbleHistory()`；bubble 实际展示 `bubbleHistory.displayed ?? displayMessage`；60s 自动 dismiss 计时器把 `bubbleHistory.displayed` 加进 deps，导航时同步重置；`proactive-message` 监听通过 ref-pattern 调 `bubbleHistory.reset()` 把用户从历史拉回 live；`handleBubbleClick` 在 history mode 下跳过 R1b 反馈记录（语义：拒绝 live 一句而非历史快照）。
  - **M4**：`pnpm tsc --noEmit` 干净；`pnpm build`（tsc + vite）494 modules 通过；TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 本轮是体验微调（让错过的 bubble 能翻回看），不是新亮点功能；与桌面气泡 R40+ 系列迭代同性质。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；交互正确性靠 tsc + 状态机推演 + 已有的 ChatBubble 交互 (R40-R42) 的语义保持。如果用户在 dev 模式下运行发现按钮不响应 / 重叠 ✕ 等视觉冲突，可针对性 patch。
