# 抽 `useThemeChangeSync` hook

## 背景

3 个窗口入口（App.tsx pet 窗 / DebugApp.tsx 调试窗 / PanelApp.tsx）都 listen
`theme-change` 和 `accent-change` 事件做跨窗口 CSS var 同步。App.tsx + DebugApp.tsx
两份完全相同（25 行 listen + dedup + applyTheme），第 3 份 PanelApp.tsx 因为还
有本地 setTheme React state 逻辑略有不同（不能复用）。

抽 hook 合并前两份。

## 改动

### `src/hooks/useThemeChangeSync.ts`（新）

```ts
export function useThemeChangeSync(): void
```

实现：useEffect 异步 listen 两条事件；接 dedup（与既存 storage 值比对）+
applyTheme + setStoredTheme/Accent；cleanup 双 unlisten。

### `src/App.tsx`

```ts
useThemeChangeSync();
```

替换 25 行内联 effect。删 `listen` / `setStoredTheme` / `setStoredAccent` /
`Accent` 等不再使用的 import。

### `src/DebugApp.tsx`

同上替换；额外清掉 useEffect import（DebugApp 内 useThemeChangeSync 后就没
其它 useEffect 了）。

### PanelApp.tsx

不动。它的 listener 内嵌入 React `setTheme` setter，与 hook 的"无本地 state"模型
不一致；抽出来反而需要给 hook 加可选 setState 回调，比就地维护更绕。

## 不做

- 不动 PanelApp.tsx：模式不同（emit + 持本地 state）
- 不写 vitest：项目无 frontend test runner

## 验收

- `npx tsc --noEmit` ✅
- PanelApp 切换主题 / accent → App pet 窗 + Debug 窗都同步切换
- 不同窗口同时打开时不出 emit 回环（hook 内部 storage 值 dedup 守门）

## 完成

- [x] hooks/useThemeChangeSync.ts 新建
- [x] App.tsx 替换 inline effect + 清未用 import
- [x] DebugApp.tsx 同上 + 清 useEffect import
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
