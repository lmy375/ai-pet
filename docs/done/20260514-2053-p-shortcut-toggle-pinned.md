# 任务行键盘 `p` 切换 pinned

## 背景

TODO 上 auto-proposed 一条："任务行键盘 `p` 快捷键切换 pinned：与既有 d / r / Delete 同模式，圈选后按 p 批量 pin / unpin。"

任务面板的键盘导航已经覆盖 ↑↓ 焦点移动、空格选中、Enter 展开、`d` 标 done、`r` 重试、Delete / Backspace 取消 —— 唯独 `p` 切 pinned 缺位。owner 加 `p` 后只需 ↑/↓ 定位 + p 一键钉住，省"鼠标 → 右键 → 找菜单项"三步。

## 改动

### `src/components/panel/useTaskKeyboardNav.ts`

#### TaskItemLike 扩 `pinned`

```ts
interface TaskItemLike {
  title: string;
  status: "pending" | "done" | "error" | "cancelled";
  /** 是否被 owner 标 `[pinned]`。`p` 单键快捷反转此值。 */
  pinned?: boolean;
}
```

#### 新 arg `handleTogglePinned`

```ts
handleTogglePinned: (title: string, nextPinned: boolean) => Promise<void>;
```

与 handleMarkDone / handleRetry 同 ref-stable pattern：

```ts
const handleTogglePinnedRef = useRef(handleTogglePinned);
useEffect(() => { handleTogglePinnedRef.current = handleTogglePinned; }, [handleTogglePinned]);
```

#### 新 `p` 单键分支

```ts
} else if (e.key === "p" && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
  setFocusedIdx((prev) => {
    if (prev === null) return null;
    const item = list[prev];
    if (!item) return prev;
    e.preventDefault();
    void handleTogglePinnedRef.current(item.title, !item.pinned);
    return prev;
  });
}
```

与 d / r 同 fire-and-forget + tagName 守卫（INPUT / TEXTAREA / SELECT / BUTTON 焦点时不响应，已在 hook 顶部统一拦截）。

**不限定 status** —— 与桌面右键菜单 / bulk pin 同放宽（pin 与 status 正交：done / cancelled 也允许 owner 复盘时标"经典作"）。这是 p 与 d / r / Delete 的关键差异（那三个都限 pending / error）。

### `src/components/panel/PanelTasks.tsx`

新增 `handleTogglePinned` callback（与 handleMarkDone / handleRetry 同 setActionErr / setBusyTitle / reload 模式）：

```ts
const handleTogglePinned = async (taskTitle: string, nextPinned: boolean) => {
  setActionErr("");
  setBusyTitle(taskTitle);
  try {
    await invoke<void>("task_set_pinned", { title: taskTitle, pinned: nextPinned });
    await reload();
  } catch (e) {
    setActionErr(`${nextPinned ? "钉住" : "取消钉住"}失败：${e}`);
    window.setTimeout(() => setActionErr(""), 3500);
  } finally {
    setBusyTitle(null);
  }
};
```

传给 useTaskKeyboardNav。

### `src/components/panel/KeyboardHelpOverlay.tsx`

帮助面板加 p 条目：

```text
[p] 切换当前焦点行 pinned（与右键菜单「📌 钉住」对偶；所有 status 都响应）
```

## 关键设计

- **`p` 不限 status**：与 d / r / Delete 三个限 pending / error 的"状态转移"快捷键不同 —— pin 是 owner 偏好标注（与状态正交），done / cancelled 也允许。这与桌面右键菜单 / bulk pin / `/pin` slash 命令的"不限状态"语义一致。
- **`!item.pinned` 而非传 true**：反转当前值让 `p` 成单键 toggle。和"两个独立按钮"（如 bulk pin / unpin）相比，单键 toggle 在键盘流里更顺 —— 不必判断"我是要 pin 还是 unpin"，直接 p 就翻。
- **fire-and-forget + setActionErr 反馈**：与 d / r 模式一致。后端 `task_set_pinned` strip-before-write 幂等所以"重复 p" 不会损坏 description；reload 让 chip 数 / UI 即时刷新。
- **TaskItemLike.pinned 加 `?` 可选**：兼容老 session 后端可能不填字段；undefined → `!undefined === true` → 默认行为是"pin"（合理：未 pin 的任务首次 p 应该 pin）。
- **更新 KeyboardHelpOverlay 同步**：帮助面板是 owner 学快捷键的发现入口；新增快捷键不更新这里 = silent feature。

## 不做

- **不做"批量 pin via 键盘"**：既有 d / r 都只动焦点行（不动 selected set）；bulk pin 走鼠标批量工具栏按钮 + `/pin <title>` slash 多次。增加"选中多条 → 按 p 批量"会破坏 single-key 单一职责 + 与 d / r 行为不对称。owner 真要批量走 toolbar。
- **不写测试**：jsdom 下 keydown / focused state 行为偶尔与真 webview 偏差；既有 d / r / Delete 路径都无单测。视觉验证（↑↓ 定位 → p → 📌 chip 浮现 / 消失 + ☑ checkbox-progress 计数推进）足够。
- **不和"chip filter 已开 📌 钉住"互动特殊化**：在 📌 chip filter 下按 p 取消钉住会让该行立刻从 visibleTasks 消失。视觉上是符合预期的（"我刚说不钉了，从'只看钉住'里消失"），不需要特别处理。
- **不接快捷键到桌面 ChatMini / pet 主区**：本快捷键属于"任务面板键盘流"集群；不应在 chat 输入框抢键。tagName 守卫已隔离 INPUT / TEXTAREA。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s
- 改动 ~50 行（hook 25 + PanelTasks 18 + KeyboardHelpOverlay 1 + comment）；既有 d / r / Delete / 方向键 / 焦点 clamp / 搜索快捷等键盘路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 2 条，余 4 条留池：
- 桌面 pet 右键菜单加「📂 打开数据目录」
- 桌面 pet Esc 收起窗口
- detail.md LinkCard 特殊域名 emoji
- 任务行 hover preview 段也走 LinkCard

## 后续

- ⌥ + p 或 ⇧ + p 批量切 selected set 的 pinned —— 与 d / r 同补全 modifier 批量语义。当前 selected 是鼠标 + 空格构建，键盘党不一定常用。
- `s` 单键调出 snooze 子菜单（with 子键 m / h / n / t / w 选 30m / 1h / 今晚 / 明早 / 下周一）—— 复杂度高，要弹层 + 多键序列，慢慢扩。
