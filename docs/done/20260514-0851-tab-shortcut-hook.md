# 抽 `useTabKeyboardShortcut` hook

## 背景

PanelApp + DebugApp 各自内联实现了 `⌘1` – `⌘N` 跳 tab 的 useEffect（~17 行同形结构，只 tabs 数组 + 键位范围有别）。抽 hook 让二次复用 / 未来若再开第三个窗口的 tab 切换直接走。

## 改动

### `src/hooks/useTabKeyboardShortcut.ts`（新）

```ts
export function useTabKeyboardShortcut<T extends string>(
  tabs: ReadonlyArray<T>,
  setActiveTab: (t: T) => void,
): void
```

实现：keydown effect 拦截 `metaKey || ctrlKey` + 数字 1-9，自动按 `tabs.length` 决定上限；INPUT / TEXTAREA / contenteditable 聚焦时让出键位；preventDefault 防 webview 默认。

### `src/PanelApp.tsx`

```ts
useTabKeyboardShortcut(TABS, setActiveTab);
```

替换 ~17 行内联 effect。

### `src/DebugApp.tsx`

同样替换。

## 不做

- 不在 hook 里加可选 `disabled?: boolean`：当前两个 caller 都全程启用
- 不暴露自定义 key 范围：1-9 上限够覆盖任何合理 tab 数

## 验收

- `npx tsc --noEmit` ✅
- PanelApp `⌘1`-`⌘5` / DebugApp `⌘1`-`⌘4` 行为不变
- 在 input 聚焦时按 `⌘<digit>` 不切（保留打字）

## 完成

- [x] hooks/useTabKeyboardShortcut.ts 新建
- [x] PanelApp.tsx: 替换 inline effect
- [x] DebugApp.tsx: 替换 inline effect
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
