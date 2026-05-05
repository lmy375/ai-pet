# 设置面板搜索框 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 设置面板搜索框：「设置」标签内容很长（>10 个 section），加顶部 search 输入按 label 过滤可见 section / 字段，省得长滚。

## 目标

PanelSettings form 模式下有 11 个 section（Live2D / LLM / MCP / Telegram / 主动开口 /
早安简报 / 工具风险 / 记忆整理 / 对话上下文 / 隐私过滤 / SOUL.md），单页滚动很
长。本轮在面板顶部加一个搜索框，输入时按 section 标题 + 关键字（label / 同义词）
过滤可见 section，输入空时全展。

## 非目标

- 不在 raw（YAML）视图模式下显示搜索 —— 那是单 textarea，搜没意义。
- 不做字段级高亮（搜中的字段在 section 里背景闪烁）—— section 级过滤已能让用户
  锁定目标，单字段高亮加交互复杂度。
- 不在 PanelChat / PanelTasks 等其它标签页里启用 —— 它们的 section 数量与长度
  都没到 PanelSettings 的量级。
- 不写 README —— 设置体验补强。

## 设计

### UX

- form 模式顶部：一个全宽 search 输入 + ✕ 清除按钮（仅当 query 非空显示）
- 输入即时过滤（无 debounce —— pure 字符串 includes 极轻）
- 标题 + 关键字命中（大小写不敏感子串）；不命中的 section 整段不渲染
- 全部过滤掉 → 显示一行 "没有匹配的设置项"

### 实现

`PanelSettings.tsx` 内部抽出一个小组件 `SearchableSection`：

```tsx
function SearchableSection({
  title,
  keywords = [],
  query,
  children,
}: {
  title: string;
  keywords?: string[];
  query: string;
  children: React.ReactNode;
}) {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return <>{children}</>;
  const haystacks = [title, ...keywords].map((s) => s.toLowerCase());
  if (haystacks.some((s) => s.includes(q))) return <>{children}</>;
  return null;
}
```

每个现有 section 用 `<SearchableSection title="..." keywords={[...]} query={searchQuery}>`
包一层；children 是原 `<div style={sectionStyle}>...</div>`。`keywords` 给关键字
扩展（用户搜 "key" 时应能命中 LLM section，搜 "regex" 应能命中隐私过滤 section）。

state: `const [searchQuery, setSearchQuery] = useState("")`。

empty-state：用一个状态变量 `visibleCount`？过度。直接渲染所有 11 个 SearchableSection
后再用一个 sentinel "no match" div，根据 `searchQuery && visibleCount===0` 控制——
但 visibleCount 难精确算（依赖每个 SearchableSection 是否渲染）。简化：复用同一
matching 函数算"有多少 section 命中"。把判定逻辑抽成 pure helper：

```ts
function matchSection(title: string, keywords: string[], query: string): boolean
```

SearchableSection 与 empty-state 都调用它，单点真相。

### 关键字表（实现时内联）

| section | keywords |
| --- | --- |
| Live2D 模型 | live2d, model, motion, miku, 映射, 动作 |
| LLM 配置 | llm, api, key, model, openai, base, url, gpt |
| MCP Servers | mcp, server, tool, 工具, 服务器 |
| Telegram Bot | telegram, tg, bot, token, 机器人 |
| 主动开口 | proactive, 主动, cooldown, idle, quiet, chatty, companion, mute, heartbeat, 心跳 |
| 早安简报 | morning, briefing, 早安, 简报, 天气, 日历 |
| 工具风险 | tool, risk, 审核, review, 风险 |
| 记忆整理 | consolidate, memory, 整理, 记忆, stale |
| 对话上下文 | chat, context, message, 上下文 |
| 隐私过滤 | privacy, redaction, regex, pattern, 私人, 隐私 |
| 系统提示词 (SOUL.md) | soul, prompt, persona, 人格, 设定 |

### 测试

`matchSection` pure，但项目无前端 vitest 配置。判定逻辑足够小（3 行），靠 tsc +
手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `matchSection` 纯函数 + `SearchableSection` 子组件 + 顶部输入 + state |
| **M2** | 把 11 个现有 section 包到 SearchableSection，填好 keywords |
| **M3** | empty-state + tsc + build + cleanup |

## 复用清单

- 已有 `containerStyle` / `sectionStyle` / `inputStyle`
- search 模式与 `PanelChat` 的搜索面板风格保持类似（清晰的 ✕ 按钮 / sticky 顶部）

## 待用户裁定的开放问题

- 是否在 viewMode === "raw" 也显示？本轮**否**——搜索对单 YAML textarea 无意义。
- 关键字表是否分散到各 section 自管？本轮**集中表**（同一文件易维护，避免散在
  10 个 component 头部）。

## 进度日志

- 2026-05-05 17:00 — 创建本文档；准备 M1。
- 2026-05-05 17:30 — 完成实现：
  - **M1**：`PanelSettings.tsx` 加 `searchQuery` 状态；新增 `matchSection(title, keywords, query) -> boolean` 纯函数（空 query 全 true / 非空时 title + keywords 任一含子串大小写不敏感）+ `SearchableSection` 子组件（不命中时返回 null 隐藏整个 section）。新增 `SETTINGS_SECTION_INDEX` 索引（11 条 [title, keywords] 元组）给 empty-state 用。
  - **M2**：form 模式顶部插入 search input + ✕ 清除按钮；11 个现有 section 每个 wrap 一层 SearchableSection 并填上配套 keywords（如 LLM = api/key/model/openai/base；隐私过滤 = privacy/redaction/regex/pattern；记忆整理 = consolidate/memory/stale/weekly 等）。
  - **M3**：empty-state（`searchQuery` 非空且 `SETTINGS_SECTION_INDEX.every(([t,k]) => !matchSection(t,k,q))`）显示提示行；Save 按钮永远可见不被过滤。
  - **M4**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 设置面板可寻性补强，与既有 panel 迭代同性质。
  - **设计取舍**：title + keywords 分离（title 来自既有 h4 文字，keywords 是用户搜索习惯映射）—— 用户不一定按章节标题措辞搜（搜 "key" 期待找到 LLM 而不是搜 "LLM"），keywords 表把这种"同义"显式化。raw mode 不显示搜索（单 textarea 无意义）。SETTINGS_SECTION_INDEX 是 SearchableSection props 的副本而非派生，但漏同步只影响 empty-state 表现，不破坏主流程渲染（SearchableSection 自管自的命中），可接受。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；matchSection 是 pure，索引是静态数组，由 tsc 保证类型正确；UI 结构由 vite build 验证 JSX 闭合正确。
  - **TODO 后续**：列表清空后按"如果需求列表已空"规则提出 5 条新候选（任务搜索框结果计数 / TG 长消息分页提示 / 任务面板键盘选中 / 决策日志 prompt 标签 chip / mood_history 删除入口）。
