# 决策日志按时间反向滚动开关 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志按时间反向滚动开关：现按"最新在底部"展示，需要往下滚才能看最新；加一个开关让用户切换"最新在顶部"，与多数日志面板的直觉对齐。

## 目标

`PanelDebug` 决策日志当前按"最新在底部"展示，与终端 / shell 日志风格一致
但与多数浏览器 devtools / dashboard 面板（最新在顶部）相反。每次打开面板
都要往下滚到最新，复盘"为什么这小时一直 Skip"很累。本轮加一个排序开关，
点一下切换"最新在顶部"。

## 非目标

- 不持久化到 localStorage —— PanelDebug 的其它过滤态（kind chips / reason
  search）都没持久化，这是临时 debug 视角；保持一致更省心智。
- 不动 in-memory 决策日志的存储顺序 —— 排序是渲染层关心的事，存储仍按
  ring buffer 自然时间序，避免影响后端 `get_proactive_decisions` API 语义。
- 不做"自动滚到底部 / 顶部" 行为 —— 用户切换排序后视觉位置已经变化（最
  新条目从底→顶），强制滚动反而打断阅读位置；让 maxHeight overflow 容器
  保持原 scrollTop 即可。

## 设计

### state

`decisionsNewestFirst: boolean` default false（保留现有行为，避免老用户
被突然颠倒搞糊涂）。与 `decisionFilter` / `decisionReasonSearch` 同级。

### UI

排序按钮在已有 filter 行（chips + search）的最末尾，与 reason 搜索 ✕
clear 按钮同级别。文案 + 视觉：
- 默认（最新在底）："↓ 最新在底"
- 切换后（最新在顶）："↑ 最新在顶"

按钮样式与 reason clear ✕ 同款（`fontSize: 10`, gray border）—— 保持
filter 行视觉一致。

header 行 "最近 N 次主动开口判断（最新在底部）" 文案随开关变化：
"（最新在顶部）" / "（最新在底部）"，让用户一眼确认当前状态而不是只看
按钮。

### 渲染

现有 `filtered` 在 if/return 之前已经计算好；切换时 `displayed = newestFirst
? [...filtered].reverse() : filtered`。`reverse()` 在 7-day 容量上常数级、
不影响性能。

## 测试

`PanelDebug` 是 IO 重的容器；前端无 vitest。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + 文案动态切换 |
| **M2** | 渲染 reverse + filter 行末按钮 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 filter chip 行容器
- 既有 ✕ clear 按钮视觉
- 既有 `filtered` 计算

## 进度日志

- 2026-05-06 19:00 — 创建本文档；准备 M1。
- 2026-05-06 19:05 — M1 完成。`decisionsNewestFirst` state default false（保留 ring-buffer 自然时序）；header 文案动态切换「最新在顶部 / 底部」。
- 2026-05-06 19:15 — M2 完成。filter 行末加 `↑ 最新在顶 / ↓ 最新在底` 切换按钮；renders `[...filtered].reverse()` 当 newestFirst；`displayed` 替换 filtered 跑空态判定 + map。
- 2026-05-06 19:20 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 930ms)。归档至 done。
