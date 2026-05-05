# mood_history 当日 entry 列表导出 Markdown — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood_history 当日 entry 列表导出 Markdown：drill 页面加 "复制为 MD"，输出 `### YYYY-MM-DD\n- HH:MM [motion] text`，便于把"今天为什么这么躁"贴笔记复盘。

## 目标

mood sparkline 已支持 drill 当日 entries + motion chip 过滤。但用户回顾完
"原来周三全是 Flick3"想把这条记录贴进笔记 / 周记复盘，仍要一条一条手动
拼。本轮在当日详情头部加 "复制为 MD" 按钮，一键拼好整段 markdown 写到
剪贴板。

## 非目标

- 不导出 7 天全量 —— sparkline 主轴自带 7 天聚合 chip；用户想复盘"过去一周
  情绪走向"应该回顾的是聚合趋势而非逐 entry 流水。
- 不让 entryFilter 影响导出 —— filter 是临时透镜（"我先看看 Flick3 几次"），
  不应改变"复制走的是当日完整记录"的语义。导出始终是 dayEntries 全集。
- 不输出 motion 中文标签 —— `Flick3` 比 `焦虑/烦躁` 在笔记里更稳定（中文
  描述以后可能演进，方括号 motion 名是接口语义）；想要中文用户可以自行
  增添。

## 设计

### 输出格式

```markdown
### 2026-05-05
- 14:30 [Flick3] 不知道为什么有点烦
- 14:31 [Tap] 看到喵了
- 23:50 [Idle] 困了
```

要点：
- `###` 三级标题：让用户贴进 `##` 二级章节下时自然嵌套。
- 每条 `- HH:MM [Motion] text`：HH:MM 与既有 UI 一致（取 timestamp[11..16]）；
  motion 用方括号包；text 原样。
- 当日 0 entry 时按钮整个不渲染（与既有"当日没有 mood_history 记录"分支
  对应；按钮无意义不展示）。

### 纯函数

`formatDayEntriesAsMarkdown(date: string, entries: MoodEntry[]): string` 放在
`MoodSparkline` 上方的 utils 区。entries 空时返回 `### {date}\n- (空)` —
但 caller 应在空时不渲染按钮，所以这个 fallback 主要给将来万一有别处复
用。

### UI

在当日详情头部行（`{selectedDate} · 当日 N 条 mood entry`）右侧、关闭
按钮 ✕ 之前，插一个 "复制为 MD" 按钮。复用既有「点了变绿 + 1.5s 自动复
位」ack 机制（仿照 PanelTasks 的 copiedDetailKey 模式），但只一个点位 →
state 直接用 `copiedDayMd: boolean` 比 keyed Map 更轻。

## 测试

`MoodSparkline` 是容器组件，无 vitest。`formatDayEntriesAsMarkdown` 是纯字
符串拼装，复杂度低，工作量不大但前端无测试框架；靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `formatDayEntriesAsMarkdown` 纯函数 |
| **M2** | `copiedDayMd` state + 头部按钮 + clipboard write |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `dayEntries` / `selectedDate` state
- 既有 `navigator.clipboard.writeText`
- 既有 ack 视觉（绿字 / 1.5s 复位）

## 进度日志

- 2026-05-06 14:00 — 创建本文档；准备 M1。
- 2026-05-06 14:10 — M1 完成。`formatDayEntriesAsMarkdown(date, entries)` 纯函数加在 MoodSparkline 上方；空 entries 兜底输出 `### date\n- (空)`。
- 2026-05-06 14:20 — M2 完成。`copiedDayMd` state + selectedDate change useEffect 一并 reset；按钮在 dayEntries.length > 0 时插入头部行右侧（关闭按钮之前），点击 → clipboard.writeText → 1.5s 绿字 ack。
- 2026-05-06 14:25 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 924ms)。归档至 done。
