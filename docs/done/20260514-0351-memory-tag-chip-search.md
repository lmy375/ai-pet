# PanelMemory tag chip 点击 → 预填搜索框

## 背景

上一轮加了 inline `#tag` chip 但只是装饰，无交互。chip 自带"分类感"但点不动 — 用户看到 #weekly 想"展开所有 #weekly 条目"必须手敲到搜索框。

最小可用：chip 点击 → 把 `#tag` 写到顶部搜索 input + 聚焦它，让用户按 Enter 真搜索。不直接执行搜索是为了让用户能改（如 `#weekly` → `#weekly 报告`）；focus + 预填已经是 80% 自动化。

## 改动

`src/components/panel/PanelMemory.tsx`：tag chip 从 `<span>` 改为 `<button>`，加 `onClick` 与 hover 提示：

```tsx
<button
  key={t}
  type="button"
  onClick={() => {
    setSearchKeyword(`#${t}`);
    searchInputRef.current?.focus();
  }}
  style={{
    fontSize: 10,
    padding: "1px 6px",
    borderRadius: 4,
    background: "var(--pet-tint-purple-bg)",
    color: "var(--pet-tint-purple-fg)",
    border: "1px dashed var(--pet-tint-purple-fg)",
    cursor: "pointer",
    fontFamily: "inherit",
  }}
  title={`点击预填搜索框 #${t}（再按 Enter 搜）`}
>
  #{t}
</button>
```

`+N` overflow chip 仍是 span（无具体 tag 可填）。

`searchInputRef` 已经在文件里（line 430 附近，⌘F focus 用）。复用即可，无需新 ref。

## 不做

- 不直接调 handleSearch：state setter 异步，handleSearch 内 closure 拿到 stale searchKeyword；要么改成传参版要么走 setKeyword + 用户主动按 Enter。后者更轻，且让用户能改 query
- 不改 `+N` overflow span 为 button：那只是计数器，没单一 tag 可对应
- 不抽 helper：单 onClick + 3 行，inline 即可

## 验收

- `npx tsc --noEmit` ✅
- 「记忆」tab 看到任意 #tag chip → 鼠标悬停 cursor:pointer
- 点击 → 顶部搜索框被填入 `#tag`、光标已在框内、按 Enter 触发搜索

## 完成

- [x] PanelMemory.tsx: tag chip → button + onClick
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
