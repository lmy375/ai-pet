# PanelMemory butler_tasks「🔊 全部 unsilent」批量按钮（iter #380）

## Background

iter #366 加「⏸ 全部 silent 1h」按钮 — 临时 silent + 1h 自动撤回。
但 owner 通过 PanelTasks 单条菜单 / TG `/silent <title>` 等路径手
动标 [silent] 的 marker 不在 timer 撤回范围。owner 想"一键回到无静
默 baseline" 需逐条 unsilent — 烦。

本 iter 加「🔊 全部 unsilent」按钮 — nuke 清所有 [silent] marker
无论来源，与 iter #366 timer 路径对偶清理入口。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. `clearAllSilent(titles)` handler（~line 533）

```ts
const clearAllSilent = useCallback(
  async (titles: string[]) => {
    if (bulkSilentBusy) return;
    setBulkSilentBusy(true);
    for (const title of titles) {
      await invoke<void>("task_set_silent", { title, silent: false });
    }
    // 若 active bulk-silent-snapshot 存在 → 一并清 timer / state /
    // localStorage 防 timer 后续唤醒时 noop 状态不一致
    if (bulkSilentSnapshot !== null) {
      clearTimeout(bulkSilentExpiryTimerRef.current);
      localStorage.removeItem(BULK_SILENT_STORAGE_KEY);
      setBulkSilentSnapshot(null);
    }
    setBulkSilentBusy(false);
    await loadIndex();
    setMessage(`🔊 已清掉 N 条 ...`);
  },
  [bulkSilentBusy, bulkSilentSnapshot],
);
```

复用 `bulkSilentBusy` flag 防与既有 silent / release 并发；失败容
忍（per-title 错误不阻塞）。

#### 2. 按钮 UI（butler_tasks 段头，~line 4612）

紧贴 iter #366「⏸ 全部 silent 1h」/「🔊 解除 (剩 N 分)」按钮：

```tsx
{catKey === "butler_tasks" &&
  (() => {
    const silentItems = cat.items.filter((it) =>
      /\[silent\]/.test(it.description),
    );
    if (silentItems.length === 0) return null;
    return (
      <button onClick={() => void clearAllSilent(silentItems.map((it) => it.title))}>
        🔊 全部 unsilent ({silentItems.length})
      </button>
    );
  })()}
```

- 仅 silentItems.length > 0 时渲（无静默不渲 dead button）
- 与 iter #366 按钮共存：snapshot active 时 owner 仍可点本按钮立
  即清"所有"（含 timer + 手动）；handler 内部一并清掉 snapshot
  state + timer 防双重撤回
- title attribute 明示与 iter #366 对偶关系 + "无论来源"语义

## Key design decisions

- **不复用 release_active 路径**：release_active 只解 snapshot 内的
  titles；本按钮意图是 "nuke 所有现存 [silent] marker"。清 snapshot
  state 是 side-effect（避免 timer 后续 noop 引发 stale 错误日志）。
- **silentItems > 0 时才渲**：dead button 增 UI 噪音。owner 在 baseline
  态（无静默）不需要这条入口可见。
- **snapshot active 时本按钮仍渲**：owner 可能 silent_all 后又手动
  单条 silent 几个，想一键清"所有"（timer 加的 + 手动加的）。本按
  钮 handler 同时清 snapshot state 让 timer noop。
- **失败容忍 per-title**：一条 task_set_silent 失败不阻塞其余；
  message 显成功 / 失败计数。与 iter #366 release_active 同模式。
- **不引入二次确认 armed**：与 iter #366 silent 触发 / release 路径
  一致 — 操作可逆（每条 task 内 `[silent]` marker 仅是文本，再标
  回去也方便）。
- **不为单 fn 引 unit test runner**：行为是 IO + state ops 非纯函数；
  build pass + 手测足够（baseline 无静默 / 仅 timer 加的 / 仅手动加
  的 / 两者混合 四场景）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 复用既有 task_set_silent Tauri 命令
