# 桌面 pet 鼠标右键聚合菜单

## 背景

TODO 上 auto-proposed 一条："桌面 pet 鼠标右键聚合菜单：右键 Live2D 区聚合『打开 panel / 切主题 / mute N 分钟 / 重启窗口』等快捷动作（当前右键无反应）。"

桌面 pet 窗口当前右键无反应 —— 用户最自然的桌面应用交互习惯（"右键查能做什么"）被完全空置。所有快捷动作都散在：
- 打开 panel → 底部 💬 按钮 / mini chat 右上 ⛶ 角
- 切主题 → 必须进 Panel 才能切（没 mini chat 入口）
- mute → 仅 slash 命令 `/sleep`，没视觉入口
- 重启窗口 → 仅 Panel 设置页隐藏角落

聚合到右键菜单是 owner 直觉路径。

## 改动

### `src/App.tsx`

#### state + outside-close 监听

```ts
const [petCtxMenu, setPetCtxMenu] = useState<{ x: number; y: number } | null>(null);
useEffect(() => {
  if (!petCtxMenu) return;
  const close = () => setPetCtxMenu(null);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  };
  // setTimeout 0 让本次 contextmenu 完成后再挂监听，避免 "刚开就关"。
  const t = window.setTimeout(() => {
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
  }, 0);
  return () => {
    window.clearTimeout(t);
    window.removeEventListener("mousedown", close);
    window.removeEventListener("keydown", onKey);
  };
}, [petCtxMenu]);
```

#### Live2D 区 onContextMenu

```tsx
<div
  style={{ position: "relative", flexShrink: 0, height: "220px" }}
  onDoubleClick={handlePetDoubleClick}
  onContextMenu={(e) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "BUTTON" || tag === "INPUT" || tag === "TEXTAREA") return;
    e.preventDefault();
    setPetCtxMenu({ x: e.clientX, y: e.clientY });
  }}
>
```

子控件（pill / chip / mute 按钮等）右键不抢菜单，让它们仍走系统默认 / 自身行为。

#### theme import 扩

```ts
import { applyTheme, getStoredTheme, getStoredAccent, setStoredTheme } from "./theme";
```

#### 菜单 JSX（fixed-position popup）

放在 App 组件 return 末尾、`</div>` 之前：

```tsx
{petCtxMenu && (() => {
  const W = 180, H = 240;
  const left = Math.max(8, Math.min(petCtxMenu.x, window.innerWidth - W - 8));
  const top = Math.max(8, Math.min(petCtxMenu.y, window.innerHeight - H - 8));
  // ... itemStyle / hover / sep ...
  return (
    <div onMouseDown={(e) => e.stopPropagation()} style={{ position: "fixed", left, top, ... }}>
      <button onClick={() => { setPetCtxMenu(null); openPanel(); }}>📋 打开面板</button>
      <button onClick={() => {
        setPetCtxMenu(null);
        const next = getStoredTheme() === "dark" ? "light" : "dark";
        setStoredTheme(next);
        applyTheme(next, getStoredAccent());
      }}>{getStoredTheme() === "dark" ? "☀️ 切到 light 主题" : "🌙 切到 dark 主题"}</button>
      <button onClick={() => void runMute(30)}>😴 mute 30 分</button>
      <button onClick={() => void runMute(60)}>😴 mute 60 分</button>
      <button onClick={() => void runMute(0)}>☀️ 解除 mute</button>
      <button onClick={async () => { await invoke("restart_pet_window"); }}>🔄 重启窗口</button>
    </div>
  );
})()}
```

## 关键设计

- **聚合 6 个高频动作**：打开 panel / 切主题 / mute 30 / mute 60 / 解除 mute / 重启窗口。每个都是既有功能但散在不同入口；右键聚合让"我现在想做什么"成为单一发现点。
- **Live2D 区为锚 + 跳过子控件**：onContextMenu 在父 div 拦截，但子级按钮 / input 右键时让它们自己处理 —— 避免抢走 pill 上自定义右键（如 priority picker）或系统默认。
- **clamp 视口边界 (W=180 / H=240) + 8px 安全边距**：与既有 TaskCtxMenu 同模板，防菜单跑出可见区。
- **setTimeout 0 + outside mousedown close**：避免"contextmenu 同次事件捕获到 mousedown → 刚开就关"。延迟一帧挂监听是 ctx menu 通用模式。
- **theme 切换直接读 storage 翻转**：与 `applyTheme` 不经 React 渲染（CSS var 自动 propagate）的设计一致 —— 不必绑 state 让菜单 label 立即变；下次打开菜单时 `getStoredTheme()` 读新值，label 自然显示反向。
- **mute 0 = 解除而非"再 mute 0 分钟"**：与既有 `/sleep 0` slash 命令同语义，复用既有后端 `set_mute_minutes` 调用。统一不同入口的语义。
- **不写 "当前 mute 状态" preview**：菜单总显 3 个 mute 选项（30 / 60 / 解除），不依赖 mute_until 状态查询 —— 简洁、无 IPC 往返。`set_mute_minutes(0)` 在已不 mute 时也是 no-op-friendly。
- **重启窗口 muted gray + 单独段**：destructive-ish 动作（虽然不破坏数据，但用户可能误触），用 muted 配色 + sep 隔离让"破坏性"视觉上独立。

## 不做

- **不做"用户自定义菜单 item"**：等真有用户诉求再加 settings 暴露。当前 6 个聚合的覆盖率够大。
- **不在 ChatMini 区右键加菜单**：那里已有自身 ctx menu（消息右键），不应被 Live2D 菜单抢。仅 Live2D 主区响应。
- **不写测试**：纯 DOM context menu + onClick 路径；jsdom 下 onContextMenu / fixed-position layout 行为偶尔与真 webview 偏差。视觉验证（右键 → 菜单浮出 → 点 item → 动作生效）足够。
- **不动 panel window 的右键行为**：本 iter 专注 pet 窗口。Panel 已有自身 ctx menu（消息 / 任务行右键），无需新加聚合层。
- **不接 quit app**：误触代价高（关掉宠物）；用户真想退出走系统 / dock 菜单。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~180 行（state + outside-close useEffect 25 + Live2D 区 onContextMenu 10 + theme import 1 + 菜单 JSX 145）；既有 handlePetDoubleClick / openPanel / set_mute_minutes / restart_pet_window 路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 3 条（其中 1 条 stale 移除），余 3 条留池：
- 跨会话搜索结果按月份分组
- 任务详情 detail.md 内嵌 https 链接预览
- （stale 移除：PanelMemory ai_insights 复制全文按钮 —— PanelMemory line 3471 + 3505 已有 📋 / 📝 双复制路径覆盖 ai_insights 项）

## 后续

- mute 状态 chip：菜单中显"当前 mute 至 HH:MM" preview，让 owner 知道还要多久 —— 需要 muteUntil poll，复杂度可接受。
- 自定义 mute 时长输入：菜单 footer 加一行 `<input type="number">` 让 owner 自填 N 分钟 —— 当前 30 / 60 二选一够 80% 场景。
- 长按 Live2D = 触摸版右键（移动 / 触屏环境）：当前 macOS 鼠标用户已够；触屏未来再扩。
- 接打开"宠物数据目录" 进菜单：与 PanelSettings 中的同款入口对偶（owner 在桌面想直接看 memories 不必先开 panel）。
