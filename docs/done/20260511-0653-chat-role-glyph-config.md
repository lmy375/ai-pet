# ChatMini 角色 glyph 配置

## 需求

复制最近 N 条对话时（上轮做的）硬编 🧑 / 🐾 前缀。用户的 SOUL.md 可能是猫娘 /
助手 / 翻译官 / 阅读伙伴...，🐾 在这些角色下不贴切。让用户在 settings 里自定义
两字段，可以是 emoji 也可以是中文短串「我」/「猫娘」/「主人」。

## 实现

### 后端 settings

`src-tauri/src/commands/settings.rs`：

- `AppSettings` 加 `user_glyph: String` / `assistant_glyph: String`
- 默认 fn `default_user_glyph() -> "🧑"` / `default_assistant_glyph() -> "🐾"`
- `Default for AppSettings` 把这俩字段也填进去
- serde `#[serde(default = "...")]` 让旧 config.yaml 不带这俩字段时仍正常解析

### 前端 wiring

- `useSettings.ts` AppSettings interface + DEFAULT_SETTINGS 同步加两字段
- `PanelSettings.tsx` 外观 section（与 ThemeToggleRow 同一 SearchableSection）下方加 2×2 grid 输入：label + input，maxWidth 200px 让短字符串视觉收敛
- tooltip 解释默认值 + 空串走 fallback；placeholder 显示默认 emoji

### ChatMini 注入

- `ChatMini` props 加 `userGlyph?: string` / `assistantGlyph?: string`
- 组件内部 `effectiveUserGlyph = userGlyph?.trim() || "🧑"`（双 fallback 守 trim 后仍空 / undefined）
- `copyRecentN()` 用 effective 值代替硬编

### 桌面 App.tsx 透传

- 已有 `settings = useSettings().settings`；`<ChatMini userGlyph={settings.user_glyph} assistantGlyph={settings.assistant_glyph} />`

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 默认体验不变：复制 N 条仍 🧑 / 🐾
  - 设置页改 user_glyph = "我" + assistant_glyph = "猫娘" → 保存 → ChatMini 顶部 📋 弹出 → 选 N 条 → 剪贴板出"我 ...\n\n猫娘 ..."
  - 字段留空 → 双 fallback 保护，仍显默认
  - 旧 config.yaml 无此字段 → serde default 兜底，不崩

## 不在本轮范围

- PanelChat / panelChatBits.exportSessionAsMarkdown 也用硬编 🧑/🐾 —— 那是
  跨会话导出的另一条路径，本轮先 ship ChatMini 的桌面快捷复制；下版本可
  以让 exportSessionAsMarkdown 也读 settings（要么 prop 透传要么直接 invoke
  get_settings）
- 没做 glyph 预设选择器（"🧑 / 我 / 主人 / 自定义..."chip）—— 文字 input
  已经覆盖；后续如反馈再加
