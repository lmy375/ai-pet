# PanelTasks 行 hover「📂 detail N 字」chip（iter #421）

## Background

PanelTasks 任务行已有多个 hover-only chip：✏ rename / ⏱ 在队列
时长 / 📊 30 天 sparkline。owner 想看「哪些 task notes 积累深」—
即 detail.md 内容字数 — 当前要展开详情面板才能看到 status bar 的
N 字 chip。

本 iter 加 hover-only 📂 detail size chip，与 ⏱ 同 hover 触发态
共出现，复用 hover-preview 500ms 已 invoke 过的 detailMap 缓存零
额外 IO。

## Changes

### `src/components/panel/PanelTasks.tsx`（紧贴 ⏱ chip 后、📊 sparkline 前）

```tsx
{taskPreviewHoverTitle === t.title &&
  (() => {
    const detail = detailMap[t.title];
    if (!detail) return null;
    const chars = Array.from(detail.detail_md ?? "").length;
    if (chars === 0) return null;
    const label = chars >= 1000
      ? `${(chars / 1000).toFixed(1)}k`
      : `${chars}`;
    return (
      <button
        type="button"
        onClick={async (e) => {
          e.stopPropagation();
          await navigator.clipboard.writeText(
            `「${t.title}」detail.md ${chars} 字`,
          );
          setBulkResultMsg(`📋 已复制：「${t.title}」detail.md ${chars} 字`);
        }}
        title={`这条 task 的 detail.md 含 ${chars} 字符...`}
        style={dashed-chip-style}
      >
        📂 {label} 字
      </button>
    );
  })()}
```

设计要点：
- **gate by taskPreviewHoverTitle === t.title**：与 ✏ rename / ⏱
  in-queue / 📊 sparkline 同 hover state（500ms 触发），所有 hover
  chip 同时浮起视觉节奏一致
- **detailMap 缓存复用**：hover preview 同路径已经 invoke 过
  task_get_detail（line 1421 已写 detailMap），本 chip 直接读零
  额外 IO；未缓存时（detail still loading）chip 不显
- **chars === 0 时不渲**：空 detail.md 显「0 字」是 dead UI；新建
  task 都先无 notes 时不浮 chip 避免噪音
- **≥ 1000 字简写 "Nk"**：1.2k / 5.3k 比 1234 / 5345 更紧凑；
  toFixed(1) 保 1 位小数让 1.0k 与 1.9k 视觉区分明显
- **click 复制 audit token**：与 ⏱ chip click 复制 ISO 同 affordance
  pattern — 让 chip 既是显示信号也是 quick-copy 入口
- **monospace fontFamily**：与既有字数 / link 数 chip 同字体让数
  字宽度对齐 — 字数变化时 chip 宽度不抖

## Key design decisions

- **不为永久显示**：hover-only 与既有 chip 节奏一致；永久显会让
  task row 视觉密度爆表
- **unicode code points 计数**：`Array.from(s).length` 而非
  `s.length`（UTF-16 code unit），与既有 status bar 字数 chip 同
  统计语义
- **不为单 chip 引 unit test**：派生 + setState；build pass + 手
  测足够（hover 长 detail task → 看 📂 N 字 chip 出现 → hover 短
  detail / 无 detail → chip 不显 → click → 看剪贴板 + toast）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.36s)
- 后端无改动 — 复用既有 task_get_detail / detailMap 缓存
