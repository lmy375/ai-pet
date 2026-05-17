# PanelDebug 加「🧹 force consolidate」按钮（iter #456）

## Background

backend `trigger_consolidate` Tauri 命令已经 production —— PanelMemory
有「立即整理」入口调用它。但 owner 在 debug 场景（验证 prompt tweak
/ audit 行为变化 / 看 progress event 实时打印）下，每次都要切 PanelMemory
→ 立即整理 → 再切回 PanelDebug 看 logs。

本 iter 加 PanelDebug toolbar 「🧹 force consolidate」按钮 — debug 视
图就近入口，复用同后端，不等 cron 节奏。与 PanelMemory「立即整理」
等价但定位差异化：前者偏「整理记忆 cleanup」，后者偏「验证 sweep 行
为 audit」。

## Changes

### `src/components/panel/PanelDebug.tsx`

#### 1. `consolidateBusy` state + `handleForceConsolidate` handler

```ts
const [consolidateBusy, setConsolidateBusy] = useState(false);
const handleForceConsolidate = async () => {
  if (consolidateBusy) return;
  setConsolidateBusy(true);
  setDebugExportMsg("🧹 正在 force consolidate sweep…");
  try {
    const status = await invoke<string>("trigger_consolidate");
    setDebugExportMsg(`🧹 ${status}`);
  } catch (e: any) {
    const msg = String(e);
    setDebugExportMsg(
      msg.includes("用户取消")
        ? "🧹 已取消整理（已完成步骤保留）"
        : `🧹 整理失败：${msg}`,
    );
  } finally {
    setConsolidateBusy(false);
    window.setTimeout(() => setDebugExportMsg(""), 6000);
  }
};
```

- 复用既有 `setDebugExportMsg` toast 通道（与 「📋 已复制调试快照
  markdown」、「🔄 已清 N 条 finished shell task」等 message 同 slot）—
  避免新 state
- 6s toast lifetime（既有 3.5s 默认 → 6s 让 consolidate result 长文案
  ms + summary 给足阅读时间）
- 错误分支区分「用户取消」（已 cancel → 友好兜底）vs 真失败（显原因）

#### 2. toolbar 按钮（紧贴 「📸 抓快照 A」之前）

```tsx
<button
  onClick={handleForceConsolidate}
  disabled={consolidateBusy}
  style={{
    ...toolBtnStyle,
    opacity: consolidateBusy ? 0.5 : 1,
    cursor: consolidateBusy ? "default" : "pointer",
  }}
  title={consolidateBusy ? "consolidate sweep 进行中…" : "…"}
>
  {consolidateBusy ? "🧹 整理中…" : "🧹 force consolidate"}
</button>
```

icon 用 🧹（"sweep" 语义更直接）— 避免与既有 「🔄 reset ⚙️」（reset
shell stats）两个 🔄 在同 toolbar 视觉混淆。

## Key design decisions

- **复用 `trigger_consolidate` Tauri 命令**：与 PanelMemory「立即整
  理」共享同后端 — 不引入新 command；语义保一致（CANCEL_FLAG reset →
  run_consolidation → status string）
- **🧹 emoji 而非 spec 的 🔄**：spec 给 🔄 是 placeholder；PanelDebug
  toolbar 已有 「🔄 reset ⚙️」shell-stats 按钮，第二个 🔄 易混淆。🧹
  sweep 在英语社区是 consolidate 同义词 + 中文「扫除」语义匹配
- **复用 `setDebugExportMsg` toast 不引新 state**：本 panel 已有的
  3.5s toast slot 是「单一 transient status 反馈」通道；本按钮也属同
  类语义 — 共享 slot 让 PanelDebug 不为每个 action 引一个新 state slot
- **6s toast 而非 default 3.5s**：consolidate 返 status 文案较长（含
  `Consolidation finished in X ms (Y items at start) · <LLM summary>`），
  6s 让 owner 读完 LLM summary（限 160 字）；过短文案被切走
- **不弹 progress popover / 不显具体阶段**：与 PanelMemory 的全 UX
  （含 phase chip + 取消按钮 + step bar）差异化定位 — 本入口是「触发
  一下看结果」轻量入口；想看进度走 PanelMemory
- **disabled 期间禁 click + 视觉灰**：consolidate 是 LLM-bound 耗时操
  作（30s ~ 2 分），重复触发可能 race。简单 busy state 防重入；与
  PanelMemory 相同模式
- **不引 cancel 按钮**：PanelMemory 已有 cancel UI；想取消的 owner 走
  那儿（cancel_consolidate 是 process-wide CANCEL_FLAG，从任何 surface
  调都生效）
- **不写 unit test**：纯 invoke + toast 副作用；逻辑 trivial（既有
  trigger_consolidate backend tests 已覆盖 status string / cancel
  semantic）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — 复用既有 `trigger_consolidate` Tauri 命令
- 手测：PanelDebug toolbar 看「🧹 force consolidate」→ 点击 → 按钮变
  灰 + label 「🧹 整理中…」→ toast 显「🧹 正在 force consolidate
  sweep…」→ 完成后 toast 改显「🧹 Consolidation finished in X ms…」→
  6s 后 toast 消失
