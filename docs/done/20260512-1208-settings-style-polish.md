# PanelSettings 样式 token 化（UI 美化 迭代 6）

## 背景

PanelSettings 是表单密度最高的页之一，旧 style 常量混用 hardcoded hex / 小阴影 / `borderBottom` 划分内部 title —— 视觉上叠"两层框"，且 dark 主题下阴影不可见。

## 改动

`PanelSettings.tsx` 末尾 style 常量全量重写：

### `sectionStyle`
- `borderRadius` 10 → 12（与 PanelPersona Section 同节奏）
- `padding` 16/18 → 18/20
- `boxShadow` hardcoded → `var(--pet-shadow-sm)`（token 化，theme-aware）
- `marginBottom` 16 → 18

### `sectionTitle`
- 移除 `borderBottom` —— card 外框已分隔，内部再加一条线"双层框"显沉重
- 让 card 边框承担分组职责

### `inputStyle`
- `padding` 8/12 → 9/12
- 新增 `transition: border-color / box-shadow`（与全局 focus halo 顺滑衔接）

### `labelStyle`
- `marginBottom` 4 → 5，多 1px 呼吸
- `letterSpacing: 0.1`

### `btnStyle` (primary 大按钮)
- `padding` 8/24 → 9/24
- `fontWeight` 500 → 600
- `letterSpacing: 0.3`
- 加 `boxShadow: 0 3px 10px <accent 28% alpha>` —— 与 PanelChat 发送按钮同节奏

### `btnSmallStyle`
- `fontWeight` 500 → 600
- `letterSpacing: 0.2`

### `btnDangerStyle`
- hardcoded `#ef4444` → `var(--pet-tint-red-fg)`（token 化）
- `fontWeight: 600`

### `mcpCardStyle`
- `borderRadius` 8 → 10
- `padding` 10/12 → 12/14
- 加 `boxShadow: var(--pet-shadow-sm)`

### `toolBadgeStyle`
- hardcoded `#e0f2fe`/`#0369a1` → `var(--pet-tint-blue-{bg,fg})`
- `borderRadius` 4 → 999（pill 化，与 PanelTasks 迭代 3 同形）
- 加 `fontWeight: 600` / `letterSpacing: 0.2`
- 加 18% alpha fg 边框

## 不做

- 不改调用方组件结构 —— 所有改动通过 style 常量传导。
- 不动 inline override（如某些场景用 `{ ...inputStyle, flex: 1 }`），override 仍生效。
- 不写测试。

## 验收

- 「设置」tab 视觉：section 卡片更立体（12px 圆角 + shadow-sm），section title 不再底带横线；primary 按钮有 accent halo；mcp 工具 badge 是 pill 形 + tint blue token。
- 浅 / 深主题切换：阴影、tint 自动适配。
- `npx tsc --noEmit` 通过。

## 完成

- [x] 9 个 style 常量重写
- [x] 移到 docs/done/
