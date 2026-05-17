# PanelTasks 行加「🔁 复制 schedule」hover chip（iter #478）

## Background

butler_tasks 段 task 可在 description 含 schedule 族 markers：
- `[every: 09:00]` / `[every: 工作日 09:00]` / `[every: 周末 10:00]` — recurring
- `[once: 2026-05-20 18:00]` — one-shot
- `[deadline: 2026-05-25 23:59]` — deadline
- `[reminderMin: 30]` — 触发前 N 分钟预提醒

owner 想「另一条 task 也用同 schedule」时，要去 PanelMemory 找 cat 的
detail.md 抄 marker 字符串。本 iter 加 PanelTasks 行 hover-only chip
「🔁 复制 schedule」 — 一键复制本 task 的 schedule markers 拼接串。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 📂 detail size chip 之后插：

```tsx
{taskPreviewHoverTitle === t.title && !isFinished(t.status) && (() => {
  const re = /\[(?:every|once|deadline|reminderMin):[^\]]+\]/g;
  const matches = t.raw_description.match(re) ?? [];
  if (matches.length === 0) return null;
  const payload = matches.join(" ");
  return (
    <button onClick={async (e) => {
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(payload);
        setBulkResultMsg(`🔁 已复制 schedule（${matches.length} 段）：${payload}`);
      } catch (err) {
        setBulkResultMsg(`复制 schedule 失败：${err}`);
      }
      window.setTimeout(() => setBulkResultMsg(""), 2500);
    }}
      title={`复制 ${matches.length} 段 schedule marker：${payload}\n\n粘到新 task description 共用同 schedule。`}
    >
      🔁 复制 schedule
    </button>
  );
})()}
```

### Regex `\[(?:every|once|deadline|reminderMin):[^\]]+\]`

- 一次性命中所有 schedule 族 markers（含 reminderMin 兄弟）
- `[^\]]+` 非贪婪式吃 inner content；不允许嵌套 `]`（与 markers 设计约
  定一致）
- `/g` flag 让多 markers 都被捕获（pet 常一条 task 含 every + reminderMin）

### Gates

- **`taskPreviewHoverTitle === t.title`**：hover 500ms 后浮，与 📂 /
  ↗ / 📊 / ↘ / ⏭ 等 hover chip 同节奏 + 减视觉密度
- **`!isFinished(t.status)`**：done / cancelled task 的 schedule 已无意
  义（不会再 fire）；保留 chip 让 owner 复制反而误导。与 ⏭ +1d / 📌+⏰
  combo 同 gate
- **`matches.length === 0`**：非 schedule task（如 ad-hoc one-shot
  无 every / deadline）chip 隐藏避免空状态噪音

## Key design decisions

- **空格拼接 `matches.join(" ")`**：与 detail.md / task description 自
  然书写习惯一致 — `[every: 09:00] [reminderMin: 5]` 多 marker 一行。
  粘到新 task description 时无需手动调整间距
- **复用 `setBulkResultMsg` 2.5s toast**：与 📂 detail size / ↗ refs
  等 hover chip 同 toast 通道；维护 PanelTasks 内一致的反馈视觉
- **toast 显完整 payload**：让 owner 即时看到「我复制的是什么」无需粘
  出来看 — 多数 schedule marker < 80 字 toast 装得下
- **`raw_description.match(re)` 不依赖 parseButlerSchedule helper**：
  parseButlerSchedule 在 PanelMemory 内闭包定义，未 export；regex 直接
  扫 raw_description 更简单 + 不引模块依赖。逻辑只关心"marker 在哪"
  非"schedule 实际语义"，regex 足矣
- **不引入「复制 + 自动建新 task」一步流**：本 chip 是「复制到剪贴板」
  通用 utility — 让 owner 自己决定粘哪儿（新 task / detail.md /
  TG /quick / 别人 chat）。整合「+ 新建」入口耦合度过高
- **不写 unit test**：纯 regex extract + clipboard 副作用；逻辑 trivial
  （marker 语法已 production 验证）。GOAL.md "meaningful tests only"
  规则下不引装饰性测试。`tsc + vite build` clean 即够

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 纯前端 UI hover chip
- 手测：PanelTasks butler_tasks active row 含 `[every: 09:00] [reminderMin: 5]`
  → hover 500ms → chip 出现 → click → toast 显「🔁 已复制 schedule（2
  段）：[every: 09:00] [reminderMin: 5]」→ 粘到新 task description 时
  marker 完整保留
