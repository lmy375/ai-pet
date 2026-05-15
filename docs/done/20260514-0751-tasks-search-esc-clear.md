# PanelTasks 搜索框 + 归档搜索框 `Esc` 清空

## 背景

上轮给 PanelMemory 搜索框加了 Esc-清 query 的现代肌肉记忆。PanelTasks 的主搜索框 + 归档区搜索框还没有 —— 两边都只有 ✕ 按钮可点。补齐让所有 panel search input 行为一致。

## 改动

`src/components/panel/PanelTasks.tsx`：两个 input 加 `onKeyDown`：

```ts
onKeyDown={(e) => {
  if (e.key === "Escape" && query) {
    e.preventDefault();
    setQuery("");
  }
}}
```

- 主搜索：search / setSearch
- 归档搜：archiveQuery / setArchiveQuery

守门 `query` 非空时才拦截 Esc —— 空 input 时让出键位给全局 Esc 行为（PanelChat-style 全局 modal 关）。不 blur，让用户改 query 继续搜。

## 不做

- 不动 ✕ 按钮：保留鼠标用户路径
- 不动其它 input：focus / typing 已被各自的局部 keydown 接管

## 验收

- `npx tsc --noEmit` ✅
- 任务面板搜索框输入 → Esc 清空（input 仍聚焦）
- 归档区搜索框同行为

## 完成

- [x] PanelTasks.tsx: 主搜索框 + 归档搜索框各加 onKeyDown
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
