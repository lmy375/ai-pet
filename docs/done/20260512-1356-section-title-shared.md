# SectionTitle 共享组件 + PanelSettings 全量迁移（UI 美化 迭代 20）

## 背景

panel 体系各处用 `<h4 style={sectionTitle}>` 或 `<div style={s.sectionTitle}>` 渲染 section 标题，每个 panel 自带一份 `sectionTitle` 样式常量，配色 / 间距 / 字号略有差异（13/13.5/14 px 混用）。PanelPersona 迭代 2 已经把 Section 标题升级到"accent 圆点 + halo + 标题 + 副标题同基线"风格 —— 视觉效果好，但限于该面板。本轮把它抽出来跨面板共用。

## 改动

### 新建 `components/panel/SectionTitle.tsx`

API：
```tsx
<SectionTitle>config.yaml</SectionTitle>
<SectionTitle subtitle="近期主动开口节奏">陪伴时长</SectionTitle>
<SectionTitle right={<button>...</button>}>新建任务</SectionTitle>
<SectionTitle noMargin>...</SectionTitle>      // flex 行内场景
<SectionTitle divider>...</SectionTitle>       // 与内容之间加底线
<SectionTitle dot={false}>...</SectionTitle>   // 行内 mini section 不要圆点
```

特征：
- accent 8px 圆点 + 18% halo（与 PanelPersona 迭代 2 同款 — 视觉一致）
- 13.5px / weight 600 / letterSpacing 0.2
- 默认 marginBottom 12；可选 subtitle 同基线、可选 right 槽自动 marginLeft auto
- divider 模式给底线 + 8px padding（仅特殊场景使用）

### PanelSettings 全量迁移（14 处）

| 原模式 | 新 |
|--------|----|
| `<h4 style={sectionTitle}><HighlightedText.../></h4>` × 9 处 | `<SectionTitle><HighlightedText.../></SectionTitle>` |
| `<h4 style={sectionTitle}>config.yaml</h4>` | `<SectionTitle>config.yaml</SectionTitle>` |
| `<h4 style={{ ...sectionTitle, margin: 0 }}>...</h4>` (3 处) | `<SectionTitle noMargin>...</SectionTitle>` |
| MCP Servers 标题 + 内联状态 `{N/M 已连接 · ... 工具}` | 拆 `subtitle` 参数 |
| Telegram Bot 标题 + 内联运行态颜色 chip | 拆 `subtitle` 参数（保留运行 / 失败 / 未启动配色） |

副作用：
- 删掉 `sectionTitle` style 常量（再无 caller）
- 14 个标题都自动多了 accent 圆点 + halo —— 视觉立刻"分块"清晰

### 未动

- PanelMemory `s.sectionTitle` 2 处：内部含拖拽 handle + collapse 交互，layout 不是标准 h4，留下一轮单独评估
- PanelTasks `s.sectionTitle`：整段是 click-to-collapse 容器，迁移需重写 click 目标语义

## 验收

- 切到「设置」tab：每个 section 标题左侧多了一个 accent 圆点，视觉立刻分块；MCP / Telegram 标题的运行状态 chip 仍跟随。
- `npx tsc --noEmit` 通过。

## 完成

- [x] SectionTitle 新组件
- [x] PanelSettings 14 处迁移 + 旧常量删除
- [x] 移到 docs/done/
