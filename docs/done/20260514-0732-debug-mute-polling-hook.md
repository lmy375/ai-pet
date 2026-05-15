# PanelDebug muteUntil 迁到 `usePollingState`

## 背景

上次跳过 `fetchMute` 是因为 mute 切换 button 处直接 `setMuteUntil(until)` 用 set_mute_minutes 的返回值更新本地状态，省一次 get_mute_until 往返。但代价是：现在有两条更新 muteUntil 的路径（polling + button 内联），数据源不单一。

迁到 hook：button 处改为调 `refreshMute()` 触发 hook 重 fetch，所有更新走 hook 一条线。多 1 次 IPC 在 mute toggle 这种罕见动作上完全可接受，换来代码清爽。

## 改动

`src/components/panel/PanelDebug.tsx`：

- 删 `useState<string>("") setMuteUntil` + 30s polling useEffect（~17 行）
- 替换为：

  ```ts
  const { data: muteUntil, refresh: refreshMute } = usePollingState(
    () => invoke<string>("get_mute_until"),
    30_000,
    "",
  );
  ```

- mute 按钮 handler 把 `setMuteUntil(until)` 改为 `void refreshMute()`。`until` 局部变量仍用于拼 status 文案（至 HH:MM 显示），不影响。

行为变化：mute toggle 完成后多一次 get_mute_until invoke，UI 上看不到差别。失败 catch 走 hook 静默（原也是 silent）。

## 不做

- 不动 muteRemainingMins useMemo（依赖 muteUntil）：hook 数据是同名 alias，自动跟随
- 不暴露 refreshing 给 button：UI 不需要 loading 态（toggle 已有 muteBusy 守门）

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab mute 15min 按钮文案、剩余分钟、自动 30s 刷新行为不变
- 点击 toggle → 状态立即更新

## 完成

- [x] PanelDebug.tsx: muteUntil 接入 usePollingState；删旧 effect / setter；button refreshMute
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
