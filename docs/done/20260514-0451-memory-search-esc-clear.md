# PanelMemory 搜索框 `Esc` 清空

## 背景

PanelMemory 搜索框：
- `Enter` → handleSearch
- 「搜索」按钮 → handleSearch
- 「清除」按钮（仅 searchResults 非 null 时显） → 清掉 keyword + results

**缺一条键盘路径**：用户搜完想换个 query / 退出搜索态时只能伸手点「清除」按钮。`Esc` 是 Chrome / Notion / GitHub 等同款搜索框的肌肉记忆。

PanelTasks 的 search input 已经有 `Esc` 退出（见 line 3247 onKeyDown 处理）。补齐 PanelMemory 让两侧一致。

## 改动

`src/components/panel/PanelMemory.tsx`：

input 的 `onKeyDown` 从单 Enter 改为：

```ts
onKeyDown={(e) => {
  if (e.key === "Enter") {
    handleSearch();
  } else if (e.key === "Escape" && (searchKeyword || searchResults !== null)) {
    e.preventDefault();
    setSearchResults(null);
    setSearchKeyword("");
    // 不 blur：用户可能马上要换 query 继续搜；保持焦点
  }
}}
```

守门 `searchKeyword || searchResults !== null` 让纯空搜索框按 Esc 不抢全局 Esc 行为（其它 panel 全局 Esc 用于关 modal / 帮助层）。

## 不做

- 不加 inline ✕ 按钮：现有「清除」按钮在边上够用；inline ✕ 与「搜索」「清除」三按钮挤一行噪音
- 不动 datalist history popup：那是 native 元素的 Esc 行为（关 popup）由浏览器处理，与本守门分支不冲突
- 不写测试：纯 input 事件处理；无 vitest

## 验收

- `npx tsc --noEmit` ✅
- 「记忆」tab 顶部搜索框输入"todo" → Esc 清空 keyword + 保持焦点
- 搜索后切「清除」按钮的等效；状态保持焦点
- 空搜索框按 Esc → 走默认（不抢其它快捷键）

## 完成

- [x] PanelMemory.tsx: onKeyDown 加 Esc 分支
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
