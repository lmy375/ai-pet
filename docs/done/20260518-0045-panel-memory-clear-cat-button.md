# PanelMemory 「🗑 清空 cat」按钮（iter #433）

## Background

owner 想清空某段 cat 内全部 items（如 ai_insights 旧 reflect 累积太
多 / general 段杂项 brain-dump 想 reset / 测试时清掉测试 cat）当
前要：
1. 进 bulk-select mode → 全选段内 items → 「🗑 批量删除」armed →
   再点确认

四步 + bulk-select 模式可能让其它选区 stale。本 iter 加 section
header 内 「🗑 清空 (N)」按钮 — armed/confirm 模式（3s 内再点真
删）一键清整段。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. state + handler

```ts
const [clearCatArmedKey, setClearCatArmedKey] = useState<string | null>(null);
const clearCatArmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
const [clearCatBusy, setClearCatBusy] = useState<string | null>(null);

const armClearCat = (catKey: string) => { ... 3s timer reset clearCatArmedKey };
const handleClearCat = async (catKey: string) => {
  if (clearCatArmedKey !== catKey) {
    armClearCat(catKey);
    return;
  }
  // 真删：逐条 memory_edit("delete")
  for (const title of cat.items.map(i => i.title)) {
    await invoke("memory_edit", { action: "delete", category: catKey, title });
  }
  await loadIndex();
};
```

同 `bulkDeleteArmed` 模式：3s 内同 cat 再点真执行。`clearCatArmedKey`
唯一（同时只能 arm 一个 cat — 切换 cat click 时旧 timer 被 cleared
+ 新 cat armed）。

#### 2. section header 按钮（紧贴 + 新建 之前）

```tsx
{cat.items.length > 0 && (() => {
  const armed = clearCatArmedKey === catKey;
  const busy = clearCatBusy === catKey;
  return (
    <button
      style={{
        ...s.btn,
        marginLeft: 4,
        ...(armed ? { background: red-bg, color: red-fg, borderColor: red-fg, fontWeight: 600 } : {}),
        ...(busy ? { opacity: 0.5, cursor: "default" } : {}),
      }}
      disabled={busy}
      onClick={() => void handleClearCat(catKey)}
      title={armed
        ? `⚠ 再点确认：将删除 N 条 item（detail.md 文件一并删）。3 秒内有效。`
        : `清空 cat（N 条）— 临时项 cleanup。点击后需在 3s 内再点确认才真删。`}
    >
      {armed ? `⚠ 再点确认（${N}）` : busy ? `🗑 删除中…` : `🗑 清空 (${N})`}
    </button>
  );
})()}
```

设计要点：
- **gate by items.length > 0**：空 cat 无需清空按钮，避免 dead UI
- **armed 视觉红警告**：与既有 bulkDelete armed 同 tint-red 主题；
  fontWeight: 600 强调 "再点真执行" 语义
- **三态 label**：normal `🗑 清空 (N)` / armed `⚠ 再点确认（N）` /
  busy `🗑 删除中…`。owner 看 label 直接知道状态
- **`(N)` 计数 always visible**：让 owner 在点 armed 前明确知道
  会删多少条（减少误触 — 看到 50 条会停下来）
- **disabled busy**：防 double click 中途打乱

#### 3. detail.md 一并删

`memory_edit("delete")` 既有 backend 实现已含 detail.md 文件清理
+ index 更新 + SQLite mirror（对 mirrored cat）— frontend 不必关
心数据同步。

## Key design decisions

- **per-cat armed 而非全局**：同时多 cat armed 会让 UX 混乱 — owner
  误点旁边 cat 的 ⚠ 按钮也会触发删除。唯一 key 让 "arm 哪个 cat"
  明确
- **3s timer 而非 modal 确认**：与既有 bulkDeleteArmed / 单条 handleDelete
  / TG /cancel_all_error confirm token 一致 — pet 项目内部 muscle
  memory 已建立，不破坏
- **不显「确定要删 X 条？」modal**：modal 会打断 owner 流；arm
  + label 红警告已足够防误触 + 信息密度合适
- **逐条 memory_edit 而非 bulk transaction**：与 handleBulkDeleteMem
  同模式 — backend 没 bulk-delete-by-cat API，逐条是事实标准；失败
  per-step 累计不阻断后续。N 条典型 < 20 性能不是问题
- **不为单按钮引 unit test**：行为是 invoke 既有 + setState +
  setTimeout 计时；build pass + 手测足够（点 🗑 → 看变红 → 不动
  3s 后看自动恢复 → 再点 → 看真删 + loadIndex 刷视图）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.36s)
- 后端无改动 — 复用既有 memory_edit("delete") channel
