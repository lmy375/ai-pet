# 桌面 pet 窗口 Esc 快捷收起

## 背景

TODO 上 auto-proposed 一条："桌面 pet 窗口 Esc 快捷收起：textarea 不聚焦时按 Esc 触发 collapse，替代手点角落 ▶| 按钮。"

桌面 pet 窗口收起到桌边的唯一入口是 Live2D 区右上角的 ▶| 按钮。键盘党 / trackpad 用户精准点角落小按钮摩擦不小；Esc 是 "关闭 / 收起 / 退出" 的通用键盘语义（modal / picker / drawer 都遵从）—— pet 窗口应一致。

`useAutoHide.collapse()` 已稳定（idempotent slideToEdge guard）。仅需挂一个 window-level keydown 监听。

## 改动

### `src/App.tsx`

在 petCtxMenu state 定义之后挂一个 useEffect 监听全局 Esc：

```ts
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    if (hidden || petCtxMenu) return;
    const ae = document.activeElement;
    if (
      ae instanceof HTMLInputElement ||
      ae instanceof HTMLTextAreaElement ||
      (ae instanceof HTMLElement && ae.isContentEditable)
    ) {
      return;
    }
    e.preventDefault();
    collapse();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [hidden, petCtxMenu, collapse]);
```

`▶|` 按钮 tooltip 加 Esc 提示：

```ts
title="收起到桌边（也可按 Esc；mouse-enter 左侧 tab 召回）"
```

## 关键设计

- **跳过 `hidden`**：已收起时按 Esc 没意义，noop 防反复触发 slideToEdge（虽 idempotent 但避免冗余动画 / 状态比较）。
- **跳过 `petCtxMenu`**：右键菜单自带 Esc handler 关 menu；让那个独占 Esc。owner 期望先关菜单再考虑收起，与既有 ctx menu 模式一致。
- **跳过 input / textarea / contentEditable**：桌面 pet 输入框 / ChatPanel textarea / mini chat 编辑框等都可能有 Esc 行为（取消编辑 / cancel 流式 / 清查询）。让它们独占 Esc。`HTMLInputElement` / `HTMLTextAreaElement` / `isContentEditable` 三态覆盖几乎所有可编辑控件，与既有 ⌘K global picker 同 guard 模式。
- **deps 包含 hidden / petCtxMenu / collapse**：state 决策点 reactive；collapse 函数引用每次 render 新建但 useEffect 会 re-subscribe，handler 极轻不引入实际成本（一次 `addEventListener` + `removeEventListener`）。
- **`e.preventDefault()`**：吃掉浏览器默认 Esc（桌面 webview 上无 default Esc 行为，但 future-proof）。不调 `stopPropagation` 让其它合理监听（比如未来扩的全局 hotkey 监听）仍能收到。
- **▶| 按钮 tooltip 更新**：discovery —— owner 第一次 hover 按钮就能知道有 Esc 快捷键。比埋在 README 角落更被看到。

## 不做

- **不做"Esc 二次确认"二次按下才真收起**：collapse 不破坏数据 / 不可逆 —— 误触代价低（mouse-enter 左侧 tab 1 秒就能召回），二次确认是过度防御。
- **不在 PanelChat / Tasks 页绑 Esc**：那两处 Esc 早有自身语义（关 picker / 清查询 / cancel rename / 折菜单等），且 panel 窗口与 pet 窗口是不同 webview —— 这次 Esc 仅在 pet window 内生效。
- **不写测试**：纯 DOM keydown + 一个判断链；既有 useAutoHide / collapse / petCtxMenu 路径都无单测，本 listener 复用同模式。视觉验证（输入框失焦 → Esc → 宠物滑到桌边）足够。
- **不接 ChatMini 等子区**：window-level listener 跨整个 webview 一次拦截即可；不必子树多挂监听。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~25 行（useEffect 20 + tooltip 1 + comment）；既有 collapse / petCtxMenu / useAutoHide / 右上 ▶| 按钮路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 4 条，余 2 条留池：
- detail.md LinkCard 特殊域名 emoji
- 任务行 hover preview 段也走 LinkCard

## 后续

- ⌘\\ 切 collapse / 展开（系统级 toggle）—— Esc 现在只能"收"，没"展开"对偶。可加全局快捷 hotkey 让 owner 从其它 app 唤回宠物。复杂度大（需 Tauri global shortcut），等真有诉求再做。
- Esc 长按 / 双击 触发更深动作（关窗口 / 退出 app）—— 当前单击 Esc 收起足够；二次行为反而增加误触。
- 提示 KeyboardHelpOverlay 加 pet 窗口快捷键段 —— 当前帮助面板仅 panel 内 binding，pet 窗口没专属帮助层。等多个快捷键累积时再加。
