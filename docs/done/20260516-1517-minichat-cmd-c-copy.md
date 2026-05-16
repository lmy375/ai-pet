# 桌面 mini chat ⌘C 复制最近一条

## 背景

TODO 上 auto-proposed 一条："mini chat 消息按 ⌘C 复制最近一条：与既有顶部 📋 拷贝同源，让键盘党不必鼠标。"

ChatMini 顶部已有 📋 复制菜单（复制最近 3 / 5 / 10 条等多档），且既有 `copyRecentN(n)` helper + copyToast 反馈 UI。但路径是"鼠标点 📋 → 弹菜单 → 选 N → 完成"。键盘党希望"⌘C 直接拷最近一条"一步到位。

存量观察：line 287 注释 "⌘+C 快捷复制反馈：1.5s 显'已复制最近一条'小气泡" + line 1604-1628 的 toast 渲染早已实装。即视觉反馈层为 ⌘C 准备了基础设施，但实际 keydown 监听缺位。本 iter 补这个 gap。

## 改动

### `src/components/ChatMini.tsx`

紧贴既有 ⌘F 搜索 useEffect 之后追加 ⌘C 监听：

```ts
const copyRecentNRef = useRef(copyRecentN);
useEffect(() => {
  copyRecentNRef.current = copyRecentN;
}, [copyRecentN]);

useEffect(() => {
  if (!visible) return;
  const handler = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "c") return;
    // 选区非空：用户在拷选区内文本 → 走 browser native copy，不抢键
    const sel = window.getSelection();
    if (sel && sel.toString().length > 0) return;
    // 输入控件聚焦：textarea / input / contentEditable 内 ⌘C 是 native copy
    // 选区文本，不该被覆盖
    const ae = document.activeElement;
    if (
      ae instanceof HTMLInputElement ||
      ae instanceof HTMLTextAreaElement ||
      (ae instanceof HTMLElement && ae.isContentEditable)
    ) {
      return;
    }
    e.preventDefault();
    copyRecentNRef.current(1);
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [visible]);
```

`visible` 是 ChatMini prop —— panel 模式或 pet hidden 时 ChatMini 不可见，⌘C 不应触发我们的 handler；让位给应用其它部分（如 PanelChat）的 ⌘C 处理。

## 关键设计

- **三重让位**：
  - `selection 非空`：owner 选中了文本 → 让 browser native ⌘C 拷选区，与系统行为一致
  - `input / textarea / contentEditable focus`：textarea 选区 ⌘C 也是 native，不该被抢
  - `!visible`：mini chat 隐藏时彻底 disable handler
- **ref-pattern 包 copyRecentN**：copyRecentN 是 plain function 每 render 重建，读 `messages` / `effectiveUserGlyph` 等 props。若空 deps useEffect 直接捕获，闭包会持初始渲染的 stale messages；若加 copyRecentN 入 deps，每 render 都 re-subscribe window listener（小成本但不优雅）。ref 路径让 listener 只挂一次，读最新值。
- **复用既有 copyRecentN(1) + copyToast**：toast UI（top fixed 小气泡 / fade-in / 1.5s 自清 / done / err 双态）+ copyRecentN（拼 glyph + role 段）都已稳定运行。本 iter 仅补 keydown 入口，零新视觉 / 零新 helper。
- **`getSelection().toString().length > 0`**：精确检测"有可拷选区"。selection 仅 caret（光标）时 toString() 空。
- **严格 modifier `!shift && !alt`**：避免与 ⌘⇧C / ⌘⌥C（macOS 系统快捷键 / 浏览器扩展）冲突。
- **未跨窗口绑定**：仅 ChatMini scope。Panel window 独立处理自己的 ⌘C（textarea 内 native copy 即可）。Tauri 多 webview 下 window listener 不跨窗口。

## 不做

- **不写测试**：纯 keydown + selection 判断 + ref 调用；既有 ⌘F 同模式无单测；既有 copyRecentN + copyToast 路径已被 📋 菜单 ✓ 复制路径覆盖。视觉验证（pet 窗口聚焦 → 无选区 → ⌘C → toast "已复制最近回复" → 粘贴验证）足够。
- **不接桌面 ChatPanel textarea**：那里 ⌘C 已是 native textarea selection copy。复制最近消息走 ChatMini 入口 / panel 自身的右键菜单 "复制本条"。
- **不在 PanelChat 同款绑**：那里有 bubble 上 `📋 复制本条` 按钮 + 双击编辑路径，⌘C 已足够交叉覆盖；多绑反易混淆"⌘C 在 panel 干啥"。
- **不改 toast 文案为"已复制最近一条"**：existing wording "已复制最近回复" 与既有 📋 菜单的"复制最近 3 条"等措辞共用 channel；改一处会牵扯多 callsite + i18n 风险。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s
- 改动 ~45 行（ref-pattern + useEffect + 三层让位 + 注释）；既有 copyRecentN / copyToast / 📋 菜单 / ⌘F 搜索路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 3 条，余 3 条留池：
- detail.md 大纲浮窗 active heading 高亮
- detail.md preview hover heading 复制 section 按钮
- 任务详情顶部「📤 导出整体 markdown」按钮

## 后续

- ⌘⇧C 复制最近 3 条 / ⌘⌥C 复制本会话全部：modifier-key 增量。当前单键 ⌘C 已 cover 80% 场景，更高维度等用户反馈。
- toast 显复制的字符数 / 字数：让 owner 心里有数（与 detail.md 复制全文按钮的"已复制 N 字" 文案同模式）。
- mini chat 顶部 📋 菜单 tooltip 加 "⌘C 复制最近一条" hint，让 owner hover 即得知。
