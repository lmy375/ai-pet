# PanelDebug "看上次 prompt" modal "📋+ issue 模板" 按钮

## 需求

iter #191 加了"📋 全文复制"按钮，把当前 turn 的 prompt / reply / meta
/ tool calls 拼成 markdown。但提 GitHub issue 时还需要更多上下文：陪
伴天数 / proactive 出口分布 / 工具风险偏好 overrides / pending reviews
等 —— 这些已经聚集在既有的 `buildDebugMarkdownSnapshot` helper（panel
顶部"📋 复制调试快照"按钮用）。把两份资料合一让用户只点一次。

## 实现

`src/components/panel/PanelDebug.tsx` 在"📋 全文复制"按钮后插入新按钮
"📋+ issue 模板"：

- 沿用全文复制的 turn 段拼接（H1 改为 "Issue 模板 — Proactive turn 复盘"，
  PROMPT / REPLY / TOOL CALLS 加一层 ### subhead 让结构更清晰）
- turn 段后加 `---` 分隔，再 append `buildDebugMarkdownSnapshot()` 结果
  （陪伴天数 / 心情 motion 命中 / proactive 出口分布 / env 工具使用 /
  prompt tilt / tool overrides / 最近 speeches 等）
- accent 边描边的视觉强调（比普通 全文复制 button 略突出，引导用户
  在提 issue 场景下优先选这条）
- 反馈：复用 copyMsg state 2.5s toast "issue 模板已复制"
- buildDebugMarkdownSnapshot 已通过 useCallback 缓存，不重复 IO

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - modal 头部见两个 copy 按钮：📋 全文复制 / 📋+ issue 模板
  - 点 issue 模板 → 剪贴板 = "全文复制"内容 + `\n\n---\n\n` +
    `buildDebugMarkdownSnapshot()` 整段
  - 粘到 GitHub issue → 渲染层级清晰（H1 / H2 / H3 / fenced code）
  - 2.5s 反馈 "issue 模板已复制"
  - turn 段为空（进程刚启没 fire）时仍可点击 → 渲"（空）"占位 + 调试
    快照仍完整（提 "我没看到上次 prompt" 类 issue 时也有用）

## 不在本轮范围

- 没自动 redact 敏感字段（user_name / soul 中可能含个人信息）：
  redaction 是 PanelSettings 现有的"导出隐藏 user_name"切换；不在
  scope；用户提 issue 前可自查
- 没自动包当前 settings yaml：settings 文件可能很大且含 API key 段
  （即便存 keychain 也有引用）；用户更应粘修改过的具体字段而非整表
- 没集成 sessions 内容 / butler_history.log：太多 PII；issue 通常聚焦
  当前 turn 行为，过去会话不必夹带
- 没做 inline preview（弹小框先看复制内容）：复制即生效；用户在剪贴
  板看（或粘到 issue draft 看）足够

## TODO 池剩余

空。下一轮需自主提需求。
