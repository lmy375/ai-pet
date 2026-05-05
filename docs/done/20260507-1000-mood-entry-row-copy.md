# mood entry 列表条目复制 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood entry 列表条目复制：每条 entry hover 出现复制按钮，复制 `HH:MM [Motion] text` 单行到剪贴板，方便引用具体一条到聊天 / 笔记。

## 目标

drill 当日 entry 列表已支持 motion chip 过滤 + text 搜索 + 整段 Copy as
MD。但用户偶尔只想引用**单条**（"刚刚那句 17:32 [Flick3] 又在卡 PR 上"）
到聊天 / 笔记，当前要框选三个 span 抠文本。本轮在每条 entry hover 时显
"复制" 按钮，一键拼好 `HH:MM [Motion] text` 写到剪贴板。

## 非目标

- 不持久化 ack —— 复制反馈仅本次操作可见，1.5s 自动消失（与 PanelTasks /
  PanelDebug 的 copy-btn 同语义）。
- 不复制完整 RFC3339 ts —— 单行场景 HH:MM 够用；想要完整 ts 可 hover 现
  有 ts span（已有 title）或导出整段 MD（已有按钮）。
- 不批量选中复制 —— search + Copy as MD 双管已能覆盖"这段都要"用例，
  per-row + 多选会让 UI 复杂度跳一级。

## 设计

### 复用既有 ack 模式

参考决策日志 row 复制（PanelDebug 0200 那轮）：
- CSS hover-only：行 hover 时按钮 opacity 1，平时 0
- 点击 → clipboard.writeText + 1.5s 绿字"已复制"
- 单一 `copiedEntryKey: string | null` state（同时刻只一条按钮 ack 中）

### 复制格式

`{HH:MM} [{Motion}] {text}` 单行：
- HH:MM 与列表显示一致
- Motion 用接口名（`Flick3` / `Tap`）保持稳定（不本地化），与 Copy as MD
  整段格式同语义
- text 原样（含中文标点、emoji）

例：`17:32 [Flick3] PR review 卡了一周`

### CSS hover

新增 `<style>` 块（同决策日志 0200 模式）：
```css
.pet-mood-entry-row .pet-mood-entry-copy-btn {
  opacity: 0;
  transition: opacity 0.12s ease;
}
.pet-mood-entry-row:hover .pet-mood-entry-copy-btn { opacity: 1; }
```

每行外层加 `className="pet-mood-entry-row"`；按钮 className `pet-mood-entry-copy-btn`。

### 按钮位置

行尾（`flex` 容器自然 push 到右侧），与 text 之后；不挤进 ts / 颜色块 / text
三 span 之间。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` hover-only CSS + className |
| **M2** | copiedEntryKey state + 复制按钮 + 1.5s ack |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 entry 列表行 flex 布局
- 既有决策日志 row hover-only CSS 模式
- 既有 navigator.clipboard.writeText 路径

## 进度日志

- 2026-05-07 10:00 — 创建本文档；准备 M1。
- 2026-05-07 10:10 — M1 完成。`<style>` 块注入 `.pet-mood-entry-row .pet-mood-entry-copy-btn` hover-only opacity 切换（同 PanelDebug 决策日志 row 模式）。
- 2026-05-07 10:15 — M2 完成。`copiedEntryKey` state；selectedDate 切换 useEffect 一并 reset；行尾按钮 ack 1.5s 绿字"已复制"复位；copied 态绕过 hover-only 让 ack 持续可见。
- 2026-05-07 10:20 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 942ms)。归档至 done。
