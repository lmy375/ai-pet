# PanelTasks 行加「📜 复制 raw」hover chip（iter #489）

## Background

PanelTasks 既有 row hover chip 族（📂 detail size / ↗ refs / 📊
sparkline / ↘ expand-detail / ⏭ +1d / 🔁 copy schedule / 📅 due
countdown）覆盖各类专项 copy 入口，但缺一个**整段 raw_description**
入口。debug 场景（"这条 task 实际 description 长啥样 / markers 顺序
对吗"）和 dup-task 场景（"以本条为模板建相似 task"）都需要含全 markers
的 raw 文本。

本 iter 加「📜 复制 raw」hover chip — 一键 raw_description 到剪贴板。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 🔁 复制 schedule chip 之后插：

```tsx
{taskPreviewHoverTitle === t.title && t.raw_description.length > 0 && (
  <button
    onClick={async (e) => {
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(t.raw_description);
        const chars = t.raw_description.length;
        setBulkResultMsg(`📜 已复制 raw（${chars} 字）：「${t.title}」`);
      } catch (err) {
        setBulkResultMsg(`复制 raw 失败：${err}`);
      }
      window.setTimeout(() => setBulkResultMsg(""), 2500);
    }}
    title={`复制本 task 的 raw_description（含全 markers + 正文 body，${t.raw_description.length} 字）— debug / 复制粘到新 task 建 dup / 备份 markers 场景。`}
    aria-label="copy raw_description"
    style={{ ...common chip style... }}
  >
    📜 复制 raw
  </button>
)}
```

### Gates

- **`taskPreviewHoverTitle === t.title`**：500ms hover state（与 📂 /
  ↗ / 📊 / ↘ / ⏭ / 🔁 / 📅 同节奏）— 避免 always-visible chip 视觉
  密度
- **`t.raw_description.length > 0`**：极端 empty 兜底，防空 raw 提示
  误导（正常 task 不会 empty）
- **无 `!isFinished` gate**：done / cancelled 行的 raw 也含 markers
  ([done] / [result:] / [error:] 等）— 仍是 audit 关键场景；与 📂 detail
  size chip 同 finished-allowed 设计

## Key design decisions

- **与 🔁 复制 schedule 互补不重叠**：那个仅抽 `[every:] / [once:] /
  [deadline:] / [reminderMin:]` 4 类 schedule markers；本 chip copy 整
  段 raw（含上述 markers + 正文 + [pinned] / [silent] / [snooze:] /
  [blockedBy:] / [origin:tg:…] / [result:] 等所有）。owner 想精准复用
  schedule 走 🔁；想完整 dup / debug 走 📜
- **不引「strip markers vs raw」选项**：raw view 已是默认语义；想要
  stripped 走顶部既有 task body 选段拼接。两 mode 引下拉复杂度无收益
- **`setBulkResultMsg` 2.5s toast 显具体内容数 + title**：与既有 📂 /
  🔁 chip 同 toast pattern — owner 即时验证复制成功 + 看到字符数粗略
  确认 raw 完整性
- **不接 ⌘C / 不抢键盘事件**：本 chip 是显式 click 入口；keyboard
  user 可走右键 ctx menu / detail 展开 / quick-add 等既有路径
- **不写 unit test**：纯 clipboard write + 字符串 length；逻辑 trivial
  （既有 🔁 复制 schedule 同算法 production 验证）。GOAL.md "meaningful
  tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 hover chip
- 手测：PanelTasks 任意 row hover 500ms → chip 「📜 复制 raw」出现 →
  click → toast 显「📜 已复制 raw（N 字）：「title」」→ 粘到 markdown
  编辑器看完整 raw_description（含 markers + body）
