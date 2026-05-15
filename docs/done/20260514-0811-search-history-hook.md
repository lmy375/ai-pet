# 抽 `useSearchHistory` 共享 hook

## 背景

上轮把 PanelTasks 加了 search history datalist，复刻了 PanelMemory 既有的同一套：
- localStorage 读初始 → array filter string only → cap 5
- push 时 trim + dedup move-to-front + cap + 写盘
- catch JSON 损坏 / 不可用 → 空数组 fallback

两份 ~30 行雷同。第 3 份很容易再来（未来 PanelChat session search 等），抽 hook 提早 dedup。

## 改动

### `src/hooks/useSearchHistory.ts`（新）

```ts
export function useSearchHistory(
  storageKey: string,
  cap = 5,
): { history: string[]; push: (kw: string) => void };
```

实现：lazy init from localStorage（容错 + 类型过滤）；`push` 通过 setState functional updater 拿前态 dedup move-to-front + cap + 写盘（容错隐私 / 配额）。`push` 走 useCallback 让调用方 deps 稳定。

### `src/components/panel/PanelMemory.tsx`

```ts
const { history: searchHistory, push: pushSearchHistory } =
  useSearchHistory("pet-memory-search-history");
```

替换原 ~30 行 useState + push fn。push 调用点 (`pushSearchHistory(searchKeyword)`) 行为不变。

### `src/components/panel/PanelTasks.tsx`

```ts
const { history: taskSearchHistory, push: pushTaskSearchHistory } =
  useSearchHistory("pet-tasks-search-history");
```

替换上轮刚加的 ~30 行内联实现。

## 不做

- 不抽 datalist 渲染：每处 input 上下文不同（PanelMemory 旁边有 search 按钮 / PanelTasks 是 live filter），抽 component 反而难调；逻辑层抽够了
- 不写 vitest：项目无 frontend test runner

## 验收

- `npx tsc --noEmit` ✅
- PanelMemory 搜索 → 入历史；PanelTasks 搜索 → 入历史，两边都从 localStorage 独立 key 持久

## 完成

- [x] hooks/useSearchHistory.ts 新建
- [x] PanelMemory.tsx: 替换 inline 实现
- [x] PanelTasks.tsx: 替换 inline 实现
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
