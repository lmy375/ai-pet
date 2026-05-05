# 跨会话聊天搜索 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 聊天会话搜索：panel「聊天」加跨会话关键字搜索框，按命中片段跳转到对应会话与时刻。

## 目标

「聊天」面板现在切会话只能点会话标题（最多看到 created_at）—— 用户想找几周前那条
"我让宠物订餐"的对话只能逐个点开翻消息。本轮加一个跨会话关键字搜索：
1. 输入关键字 → 列出全部会话里命中的消息片段 + 它来自哪个会话 + 命中时刻
2. 点结果 → 切到对应会话 + 滚动到命中那条消息 + 短暂高亮

## 非目标

- 不做正则 / 模糊 / 分词 —— substring + case-insensitive 已覆盖 95% 实际查询场景。
- 不做异步索引 / SQLite / 全文倒排表 —— 典型 < 100 会话 × < 200 items，每次查询全
  扫的 O(n) 仍 < 100ms，引索引层是过早优化。
- 不在结果里展示 tool call 信息 —— 只搜 user / assistant 文字内容，工具 args/result
  噪音大。
- 不写 README —— 「聊天」标签的内嵌搜索补强，与 R 系列 panel 迭代同性质。

## 设计

### 后端

`commands/session.rs` 新增：

```rust
#[derive(Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub session_title: String,
    pub session_updated_at: String,
    pub item_index: usize,   // 在 session.items 中的索引，前端用来 scrollIntoView
    pub role: String,        // "user" / "assistant"
    pub snippet: String,     // 命中位置前后约 ~80 chars 的片段
    pub match_start: usize,  // snippet 内匹配开始的 char 位置（前端高亮用）
    pub match_len: usize,    // 匹配长度（char 数，统一 char 计而非 byte）
}

#[tauri::command]
pub fn search_sessions(keyword: String, limit: Option<usize>) -> Vec<SearchHit>;
```

实现：
1. `read_index()` 拿 session list，按 `updated_at` 倒序遍历（最近的会话先匹配，UI 列表
   顶端就是最相关）
2. 每个 session 用 `load_session` 拿 `items`
3. 遍历 items，仅 user / assistant 类型，content 做 case-insensitive 匹配
4. 命中 → 构造 SearchHit；多命中同 item 取**首个匹配**（多次出现一行只算一条；用户
   点击就跳到这行，足够 grok）
5. 总 limit 满（默认 50） → break

纯函数 `extract_snippet(content, match_start_char, match_len_char, ctx_chars)` →
`(snippet, snippet_match_start)`：按 char 计算（中文友好），匹配前后各取 `ctx_chars`，
首尾 `…` 标省略。

### 前端

`PanelChat.tsx`：
- session 标题栏右边加一个 🔍 按钮 → toggle 搜索模式
- 搜索模式开启时：
  - 隐藏 session 下拉
  - 替换为搜索面板：input + 结果列表
  - input 实时（无 debounce —— IO 廉价）调 `search_sessions`，关键字空时清空结果
  - 结果项渲染：`<role glyph> <snippet>` 一行 + 下方小字 `<session title> · <updated_at>`
  - snippet 中匹配段用浅黄 background 高亮（CSS 子串替换）
  - Esc / 清空输入 / 再点 🔍 → 退出搜索模式
- 点击结果：
  1. `setPendingScrollItemIndex(hit.item_index)`
  2. `loadSession(hit.session_id)`
  3. `setSearchMode(false)`、清空 query
  4. items 加载完后的 layout effect 检测 pendingScrollItemIndex，找
     `[data-item-idx={i}]` 节点 → `scrollIntoView({ block: "center", behavior: "smooth" })`
     → 0.5s background 高亮 → 1.5s 后清掉
- 给现有 message JSX 补 `data-item-idx={i}` 属性

### 测试

后端纯函数（`extract_snippet`、`search_session_items`）单测：
- 普通命中 / 多命中取首 / 大小写不敏感 / 空关键字 / 不命中 / 中文 substring
- snippet 边界：前段 ≤ ctx → 不加前 `…`；后段 ≤ ctx → 不加后 `…`
- match_start 在 snippet 内偏移正确（用于前端高亮）

前端无测试基础设施，靠 tsc + 手测验证整链路。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 `extract_snippet` + 全 session 搜索逻辑 + Tauri 命令 + 单测 |
| **M2** | 前端搜索面板 UI + 关键字 fetch + 结果渲染 + 高亮 |
| **M3** | 点击结果 → 切 session + 滚动到 item + 短暂 background 高亮 |
| **M4** | `cargo test` + `pnpm build` + TODO 清理 + 文档 done/ |

## 复用清单

- `commands::session::{read_index, load_session, SessionIndex, Session}` —— 直接调
- `PanelChat` 现有 `sessionDropdownStyle` 容器风格

## 待用户裁定的开放问题

- snippet 上下文长度：本轮选 80 chars 各侧 → 总长 ~160-200 chars，单行展示舒适。
- 搜索结果上限 50：实战上极少超出；超出时用户可以更精确地输入。
- 高亮颜色：黄色 (`#fef3c7`) 是搜索高亮的传统；与 panel 已有徽章 (`bg: #fef3c7,
  fg: #92400e`，priBadge) 复用。
- 是否搜 system message：不搜（system 是 SOUL.md，对用户无意义）。

## 进度日志

- 2026-05-05 05:00 — 创建本文档；准备 M1。
- 2026-05-05 05:30 — 完成实现：
  - **M1**：`commands/session.rs` 加 `SearchHit` 序列化结构 + `find_match_snippet` / `search_session_items` 纯函数 + `search_sessions(keyword, limit?)` Tauri 命令。char-based offset（中文友好），同 item 多次匹配只取首个，结果按会话 `updated_at` 倒序+default limit 50。注册到 lib.rs。新增 12 条单测覆盖：未命中 / 空关键字 / 短文本 / 中部命中前后 `…` / 边界（开头不加前导 `…`、结尾同理）/ 中文 char 偏移 / 大小写不敏感保留原 case / 跳过 tool/error 角色 / 同 item 一次输出 / 元数据携带 / 不命中。
  - **M2**：`PanelChat.tsx` 加搜索状态 + 🔍 toggle 按钮（与 session dropdown 互斥）+ 搜索面板（input + Esc 关闭 + 实时 fetch + 结果列表）。新增 `SearchResultRow` 子组件按 `match_start` / `match_len` 切三段渲染，命中段用 `<mark>` 浅黄背景。
  - **M3**：点击结果走 `handleSelectSearchHit`：必要时切会话 + `setPendingScroll(item_index)`；layout effect 在下一帧通过 `[data-item-idx="N"]` 选择器找到节点 → `scrollIntoView({ block: "center", behavior: "smooth" })` → 1.5s 临时背景高亮。`pendingScroll` 期间抑制原"加载完滚到底"自动滚动逻辑，避免视图被甩走。给所有 4 类 message wrapper（user / assistant / tool / error）补上 `data-item-idx`。
  - **M4**：`cargo test --lib` 848/848（+12）；`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 「聊天」面板的内嵌生产力补强，与既有面板迭代同性质。
  - **设计取舍**：search mode 与 session dropdown 互斥而非叠加（panel 窄，避免 UI 混乱）；无 debounce（IO 廉价 + 用户期望即时反馈）；同 item 多次匹配只取首个（点击只能跳到同行，多 hit 拆分无意义）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数链路有 12 条单测钉牢，UI 状态机由 tsc + 推演保证。
