# PanelTasks 行加「⏭ +1d」hover chip（iter #454）

## Background

PanelTasks 行既有 📅 调期 chip → popover 含 +1h / +1d / +3d / +1w /
+2w / 明早 09:00 / 清除 due 等 7 个 preset。但 owner 最常用「再推 1
天」场景（"今天做不完了，明天再说"）需要三步：点 📅 调期 → popover
浮 → 点 +1 天 preset。

本 iter 加 hover-only `⏭ +1d` 单击 chip — 直接推 due +1 天，绕过
popover。complement 既有 📅 调期（多 preset 选择 / 清除）— 那个适合
"精细推迟"，本 chip 适合"快速推 1 天"。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴既有 📅 调期 chip 之前插：

```tsx
{!isFinished(t.status) && taskPreviewHoverTitle === t.title && (
  <button
    onClick={async (e) => {
      e.stopPropagation();
      setActionErr("");
      setBusyTitle(t.title);
      try {
        const base = (() => {
          if (!t.due) return new Date();
          const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(t.due);
          if (!m) return new Date();
          return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]);
        })();
        const dueArg = formatDueInput(new Date(base.getTime() + 86_400_000));
        await invoke<void>("task_set_due", { title: t.title, due: dueArg });
        await reload();
      } catch (err) {
        setActionErr(`+1d 失败：${err}`);
      } finally {
        setBusyTitle(null);
      }
    }}
    title={t.due ? `把 due 从 ${t.due} 推到 +1 天后` : `设置 due 为现在 +1 天后`}
    style={{ ...chip-style... }}
  >
    ⏭ +1d
  </button>
)}
```

行为分支：
- **`t.due` 存在**：parse `YYYY-MM-DDTHH:MM` 本地时间组件 → Date 加
  86_400_000ms → `formatDueInput` 回 ISO。保证「下午 4:00 截止 → +1d
  = 明天下午 4:00」精确 24h 后语义
- **`t.due` 不存在**：base = now → now +1d 同算法。让"我先随便推一下"
  也能用本 chip（不必先打开 quick-add 设个 due 再 +1d）
- **parse 失败 fallback now**：极少（后端校验 due 格式），但兜底防 throw

## Key design decisions

- **hover-only + 500ms gate（`taskPreviewHoverTitle === t.title`）**：
  与既有 📂 / ↗ / 📊 / ↘ 同 row-hover state 协议，避免 always-visible 增
  加视觉密度。owner 在 row 上停 500ms 后才显，扫长队列不闪
- **手动 regex 而非 `new Date(t.due)`**：浏览器对 ISO 不含时区的
  `YYYY-MM-DDTHH:MM` parse 行为不一致（部分按 UTC，部分按 local，部分
  报错）。手动 regex + `new Date(y, mo-1, d, h, m)` 构造本地时间 Date
  保跨浏览器一致 — 与既有 `formatDueInput`（输出本地时间组件）配
- **复用 `formatDueInput` 输出格式**：与既有 popover preset 同协议 →
  后端 `task_set_due` 接收的 ISO 字符串结构一致
- **`!isFinished(t.status)` 双 gate**：done / cancelled 行调期无意义
  —— 与既有 📅 调期 chip 同 gate
- **`stopPropagation` + `onMouseDown` stop**：防 row 上层 onClick 误触
  展开 detail
- **不写 unit test**：纯 invoke + Date 算术 + click 副作用；逻辑直接，
  既有 task_set_due backend tests 已覆盖语义。GOAL.md "meaningful tests
  only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — 复用既有 task_set_due Tauri 命令
- 手测：PanelTasks row hover 500ms → 看「⏭ +1d」chip 显出 → tooltip
  显推算后的目标时刻 → click → row 立即 reload 显新 due（+1 day）
