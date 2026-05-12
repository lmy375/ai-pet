# PanelChat hardcoded hex → token 迁移（UI 美化 迭代 8）

## 背景

迭代 7 升级了 session list hover，但同行的 inline selected 高亮 `#f0f9ff` 仍 hardcoded —— dark 主题下作为 selected 行底色看起来"白条"反差刺眼。同区还有其它几处 hardcoded hex：搜索按钮 active 态 / 分隔线 / 删除按钮 / 导入 / 全清确认按钮等。

## 改动

`PanelChat.tsx`：

| 位置 | 旧 | 新 |
|------|----|----|
| 🔍 搜索按钮 active 态 fg/bg | `#0369a1` / `#e0f2fe` | `var(--pet-tint-blue-{fg,bg})` |
| Session list "全清/导入"行 borderBottom | `#f1f5f9` | `var(--pet-color-border)` |
| Session row selected bg | `#f0f9ff` | `var(--pet-tint-blue-bg)` |
| Session row 分隔线 borderBottom | `#f1f5f9` | `var(--pet-color-border)` |
| Session 删除按钮 base 态 | `#fee2e2` / `#dc2626` | `var(--pet-tint-red-{bg,fg})` |
| Session 删除按钮 armed 态 | `#dc2626` / `#fff` | `var(--pet-tint-red-fg)` / `#fff` |
| 📥 导入快照 armed | `#dc2626` | `var(--pet-tint-red-fg)` |
| 🗑 全清 armed | `#dc2626` | `var(--pet-tint-red-fg)` |

## 收益

- dark 主题下"selected session 是白条"、"删除按钮浅粉底" 等违和被修复。
- 所有 chat 内警示色统一走 `--pet-tint-red-*`，与 PanelTasks badges / PanelSettings danger 等其它面板色域一致。
- selected session 与 hover 同走 tint blue —— hover 是 8% accent / selected 是 tint blue bg，仍能区分。

## 不做

- 不动 `#fff`（armed 按钮 fg 在所有 theme 下都正确）。
- 不动 marks modal 内部一些 hardcoded —— 那是低频 modal，下次单独抽。

## 验收

- 浅 / 深主题切换 session 列表：selected 行底色自动跟随；删除按钮浅 / 深表现一致；搜索按钮 active 同。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelChat hex → tint 替换
- [x] 移到 docs/done/
