# detail.md 编辑器顶部「↑ 上 / ↓ 下一条任务」导航箭头

## 背景

TODO 上 auto-proposed 一条："PanelTasks detail 编辑器加『↑ 上 / ↓ 下一条任务』导航箭头：让 owner 不必关 detail 再开下一条，连续 review 多 task 顺手。"

owner 周末复盘 / 批量 review N 条任务的 detail.md 时，当前流程：
1. 点关 detail
2. 滚到下一条
3. 点开下一条 detail
4. （重复）

每个 task 切换 3 步。加 ↑/↓ 导航 1 步直接切下条 —— 同 IDE prev/next file 直觉。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### `handleNavigateDetail` useCallback

放在 `visibleTasks` 声明之后（TDZ 防御 —— useCallback 的 deps 数组需要 visibleTasks 已声明才能引用）：

```ts
const handleNavigateDetail = useCallback(
  async (direction: "prev" | "next") => {
    const curTitle = editingDetailTitle;
    if (!curTitle) return;
    const curIdx = visibleTasks.findIndex((t) => t.title === curTitle);
    if (curIdx === -1) return;
    const targetIdx = direction === "prev" ? curIdx - 1 : curIdx + 1;
    if (targetIdx < 0 || targetIdx >= visibleTasks.length) return;
    const target = visibleTasks[targetIdx];

    // 1. dirty 内容 sync flush 到 draft（防 60s autosave tick 没跑到丢内容）
    const dirty = editingDetailContent !== editingDetailOriginalRef.current;
    if (dirty) {
      try {
        window.localStorage.setItem(
          `pet-detail-draft-${curTitle}`,
          JSON.stringify({ content: editingDetailContent, ts: Date.now() }),
        );
      } catch (e) {
        console.error("flush draft on navigate failed:", e);
      }
    }

    // 2. 拉 target detail —— detailMap 缓存命中直接用；未命中走 IO
    let targetMd: string;
    const cached = detailMap[target.title];
    if (cached) {
      targetMd = cached.detail_md;
    } else {
      try {
        const fresh = await invoke<TaskDetail>("task_get_detail", {
          title: target.title,
        });
        targetMd = fresh.detail_md;
        setDetailMap((prev) => ({ ...prev, [target.title]: fresh }));
      } catch (e) {
        console.error("task_get_detail on navigate failed:", e);
        targetMd = "";  // 拉失败用空内容开
      }
    }

    // 3. 切换编辑器 + 滚 target row 进视野
    handleEnterEditDetail(target.title, targetMd);
    setPendingTitleFocus(target.title);
  },
  [editingDetailTitle, editingDetailContent, visibleTasks, detailMap, handleEnterEditDetail],
);
```

#### ↑ ↓ 按钮渲染

在 view-mode 切换行的 📤 export 按钮之后、📑 大纲按钮之前插入 IIFE：

```tsx
{(() => {
  const curIdx = visibleTasks.findIndex((vt) => vt.title === t.title);
  const hasPrev = curIdx > 0;
  const hasNext = curIdx !== -1 && curIdx < visibleTasks.length - 1;
  const navBtnStyle = (enabled: boolean) => ({
    fontSize: 11, padding: "2px 8px",
    border: "1px solid var(--pet-color-border)",
    borderRadius: 4,
    background: "var(--pet-color-card)",
    color: enabled ? "var(--pet-color-muted)" : "var(--pet-color-border)",
    cursor: enabled ? "pointer" : "default",
    opacity: enabled ? 1 : 0.5,
  });
  return (
    <>
      <button disabled={!hasPrev} onClick={() => void handleNavigateDetail("prev")} title={hasPrev ? `跳到上一条任务（#${curIdx} → #${curIdx - 1}）...` : "已是第一条"}>↑</button>
      <button disabled={!hasNext} onClick={() => void handleNavigateDetail("next")} title={hasNext ? `跳到下一条任务（#${curIdx} → #${curIdx + 1}）...` : "已是最后一条"}>↓</button>
    </>
  );
})()}
```

按钮 disabled 在 boundaries（第一条 / 最后一条），cursor 默认指针消失 + opacity 0.5 视觉降级。tooltip 显当前 / 目标 idx 让 owner 心中有数。

## 关键设计

- **visibleTasks 顺序作 prev/next 基准**：respects 当前 filter (search / chip / pinned) + sort (queue / due / priority) + finished 显示状态。owner 看到的列表顺序 = 导航顺序，符合直觉。隐藏的任务（被 filter 滤掉）跳过。
- **dirty sync flush 进 draft**：60s autosave tick 可能还没跑到（owner 刚打字就 click ↑/↓），所以切换前同步写一次 draft 防丢内容。owner 切回该 task 时既有"草稿恢复 banner" pipeline 自然弹出。**不**强制 auto-save 到磁盘（save 可能失败 / 慢 / 触发后端 history 写入；owner 期望 prev/next 是轻量切换，不是 commit）。
- **detailMap 缓存优先 + IO fallback**：连续 ↑/↓ 切多条任务时缓存通常命中（这些 task 之前 hover preview / expand 时已加载）。少数未命中走 task_get_detail，IO 失败 fallback 空字符串让 owner 至少能"打开个空编辑器" 而非 stuck。
- **`setPendingTitleFocus` 复用既有 pipeline**：与"完成小卡跳行" / "task ref chip click" / "搜索结果点击" 同 jump-to-task 路径 —— 清 filter + 显 finished + scrollIntoView。owner 心智模型一致。
- **boundary disabled + cursor + opacity**：第一条 / 最后一条时禁用按钮防误点。disabled HTML attr 让 click 不触发 + tooltip 显"已是第一条 / 已是最后一条"。
- **TDZ 防御**：useCallback 必须放在 `visibleTasks` / `handleEnterEditDetail` 之后。初版误放编辑器 callbacks cluster 里被 tsc 拦下，移到 line 3416 后正确。

## 不做

- **不写键盘快捷键（如 ⌘[ / ⌘]）**：要协调全局 keyboard 监听 + textarea 不抢键。当前点按钮已轻松。等用户反馈再加。
- **不写 "dirty 时弹 confirm dialog"**：60s autosave + sync flush 双 backup 已足够安全。confirm dialog 反而打断切换流。
- **不允许跨 filter 切换**：visibleTasks 顺序就是 owner 当前看到的列表，filter 收窄时 ↑/↓ 也只在收窄集合内切换 —— 与 owner 心中 "下一个我可见的 task" 直觉一致。
- **不写测试**：纯 UI button + idx 算 + IO fallback；既有 jump-to-task pipeline 路径无单测。视觉验证（开两个含 detail 的 task → click ↑/↓ → 看切换） 足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~100 行（handleNavigateDetail useCallback 60 + 两按钮 IIFE 40）；既有 view-mode row 4 按钮（📤 / 📋 / 📑 / 📂） / dirty marker / 大纲浮窗 / autosave / setPendingTitleFocus pipeline 完全不动。

## TODO 状态

6 条 auto-proposed 已完成 4 条，余 2 条留池：
- 桌面 pet hover 3s 浮 ambient 三段统计微卡片
- PanelMemory 类目 7 天 churn sparkline

## 后续

- ⌘[ / ⌘] 键盘快捷绑同样导航。
- ↑↓ 时 cancel armed 仍保 dirty-discard 二次确认 —— 当前 dirty 走 draft flush 不会丢，但 owner 想"真的丢"无入口。可加 ⇧ + ↓ 显式 "discard + 下一条"。
- 导航时 toast "→ task 「X」"反馈，让 owner 视觉确认目标。
