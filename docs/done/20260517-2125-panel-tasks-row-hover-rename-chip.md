# PanelTasks 任务行 hover ✏ rename action chip（iter #397）

## Background

task title rename 当前唯一入口是双击 title — 双击是隐藏操作，新
owner / 触屏党难发现。已有 hover preview tooltip（iter #376）解释
了"鼠标停留 0.5s 浮信息"，但 rename 仍藏在 invisible affordance 后。

本 iter 加一个显式 ✏ chip — hover row 0.5s 后浮在 title 右侧，
click 等价于双击 → 进 inline rename mode。复用既有
taskPreviewHoverTitle state（同 500ms hover trigger）保持一致。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 加 ✏ chip 在 title 行内（紧贴 isRecentlyUpdated ● 之后）

```tsx
{taskPreviewHoverTitle === t.title &&
  renamingTaskTitle !== t.title && (
    <button
      onClick={(e) => {
        e.stopPropagation();
        setRenamingTaskTitle(t.title);
        setRenameTaskDraft(t.title);
      }}
      title="改名 task title（与双击 title 等价 — Enter 提交 / Esc 取消）"
      style={{
        fontSize: 10,
        padding: "0 5px",
        marginLeft: 6,
        border: "1px dashed var(--pet-color-border)",
        borderRadius: 3,
        background: "transparent",
        color: "var(--pet-color-muted)",
        cursor: "pointer",
        fontFamily: "inherit",
        lineHeight: 1.5,
        verticalAlign: "middle",
      }}
    >
      ✏
    </button>
  )}
```

设计要点：
- **复用 taskPreviewHoverTitle state**：与 iter #376 hover preview
  tooltip 同 500ms trigger，hover 体验一致 — 0.5s 后两个 affordance
  同时出现（tooltip + ✏ chip）
- **inline 在 title 行内**：紧贴 isRecentlyUpdated ● 之后，与 title
  视觉成组（owner 看到 chip 就知道是改"这条标题"的入口）
- **dashed border + muted color**：轻视觉重量，不抢 title 文字本身
  ，chip 仅作"hover 时浮起的发现性指示"
- **renamingTaskTitle !== t.title gate**：进入 rename mode 时 chip
  自动隐藏（避免与 textarea 一起渲在同行）
- **stopPropagation**：防 click 冒泡触发 row expand
- **与双击等价行为**：click → setRenamingTaskTitle + setRenameTaskDraft，
  与既有 onDoubleClick handler 同两 setState；后续 inline rename
  textarea / Enter 提交 / Esc 取消 流程完全复用

## Key design decisions

- **不引入独立 hover state**：复用 taskPreviewHoverTitle 让两 affordance
  时序同步 — 防"tooltip 先出 vs chip 先出" 闪烁不一致。
- **chip 仅 hover 显（vs 永久显）**：永久显会让 task row 视觉密度膨胀，
  尤其窄 panel 下挤；hover 显衡量了"discoverability vs 静态简洁"。
- **不替换 onDoubleClick handler**：保留双击作为快捷键党 / 触屏党
  替代路径。新 chip 是 additive affordance。
- **位置：title 行内 vs row 右上角 absolute**：title 行内更靠近 rename
  动作目标（title 本身），心智直接；absolute 顶角是 idx counter 占
  位，加 ✏ 在那里会让两 absolute chip 视觉竞争。
- **不为单 chip 引 unit test runner**：行为是 setState 触发 + 既
  有 rename pipeline；build pass + 手测足够（hover → 看 ✏ 出现 →
  click → 看 textarea + 输入新 title → Enter 提交三场景）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
- 后端无改动 — 复用既有 inline rename pipeline
