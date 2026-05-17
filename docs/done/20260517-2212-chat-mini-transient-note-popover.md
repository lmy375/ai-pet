# ChatMini ⌘\` 弹 transient_note 快速 popover（iter #404）

## Background

ChatMini 既有 transient_note 入口三个但都偏 reactive：
- ChatBubble 右键「📝 用此话设 transient_note」(iter #377)：以 pet
  已说过的某句话作 transient（"复用 pet 文本"路径）
- PanelToneStrip ✍️ 写入口 (iter #364)：需切到 Panel 才能写
- TG `/transient <text> [minutes]`：手机端入口

owner 桌面端「我想给 pet 留个临时上下文但不想发消息打扰它」短路径
缺位 — 唯一桌面快路径要么发消息（pet 会回复打断流），要么切 Panel
（窗口切换重）。

本 iter 加 ChatMini 内 ⌘\` 弹悬浮 popover：textarea + 4 个时长 preset
（30m / 1h / 2h / 6h），写完点 → 一键挂 transient_note + 关 popover。
快捷键 ⌘\` 切换；textarea Esc 关 / ⌘Enter 默认 1h 提交。

## Changes

### `src/components/ChatMini.tsx`

#### 1. Import `useCallback`

submitTransientPopover 用 useCallback 减少子树 re-render。

#### 2. State + ⌘\` 键盘 handler

```ts
const [transientPopoverOpen, setTransientPopoverOpen] = useState(false);
const [transientPopoverDraft, setTransientPopoverDraft] = useState("");
const transientPopoverInputRef = useRef<HTMLTextAreaElement>(null);

useEffect(() => {
  if (!visible) return;
  if (!onSetTransientNote) return;
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key !== "`") return;
    e.preventDefault();
    setTransientPopoverOpen((v) => !v);
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [visible, onSetTransientNote]);
```

设计要点：
- **⌘\` 键选择**：\` 与既有 Esc / ⌘P / ⌘/ / 数字 / J/K 等不冲突；
  macOS 系统的 ⌘\` 是「下一窗口」但 webview 内不会冒到系统 — 在
  ChatMini webview 焦点内安全劫持
- **可见时才挂监听**：!visible 时 ChatMini 整体 return null（line
  981），不挂监听避免 hover-only ChatMini 隐藏时仍劫持快捷键
- **toggle 而非 open**：再按一次 ⌘\` 关闭，跳出习惯路径
- **未传 onSetTransientNote 不挂**：prop optional，未提供时整 popover
  不可用 — 不挂键盘监听避免按了无反应迷惑

#### 3. 自动聚焦 + submit helper

```ts
useEffect(() => {
  if (!transientPopoverOpen) return;
  setTimeout(() => {
    transientPopoverInputRef.current?.focus();
    transientPopoverInputRef.current?.select();
  }, 0);
}, [transientPopoverOpen]);

const submitTransientPopover = useCallback((minutes: number) => {
  const body = transientPopoverDraft.trim();
  if (!body) return;
  onSetTransientNote?.(body, minutes);
  setTransientPopoverDraft("");
  setTransientPopoverOpen(false);
}, [transientPopoverDraft, onSetTransientNote]);
```

select() 让重开时已有内容可立即覆写（"我改主意了"场景）；空 body
拒提交。

#### 4. Popover UI（fixed 居中浮窗）

```tsx
{transientPopoverOpen && onSetTransientNote && (
  <div onMouseDown={outsideClickHandler} style={overlay}>
    <div onMouseDown={stopProp} style={panel}>
      <header>📝 设 transient_note · 给 pet 写一段临时上下文（不发消息）</header>
      <textarea
        ref={inputRef}
        onKeyDown={(e) => {
          if (e.key === "Escape") setTransientPopoverOpen(false);
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submitTransientPopover(60);
        }}
        placeholder="比如：在开会，半小时别打扰 / 集中写文档别活泼 / ..."
        rows={3}
      />
      <row>
        时长：[30m] [1h] [2h] [6h] ... [✕]
      </row>
      <hint>快捷键：⌘\` 切换 · ⌘Enter 提交（默认 1h） · Esc 关</hint>
    </div>
  </div>
)}
```

设计要点：
- **fixed inset:0 半透 backdrop**：明确 modal 语义；outside-click
  关与 ctxMenu 同模式（mousedown target===currentTarget 判 outside）
- **居中浮窗 320-420px**：与既有 ChatMini 信息密度匹配 —— ChatMini
  本身可能很窄（侧栏模式），但 popover 是 fixed 居中跨整个 viewport，
  不被 ChatMini 容器宽度限制
- **4 个时长 preset 不是 input 数字**：30m/1h/2h/6h 覆盖 95% 场景；
  owner 想精确（如 90m）走 TG /transient 那条路 — popover 重在"一句话
  + 一击"快路径
- **未填文本时所有时长按钮 disabled**：防误触；title 显「请先输入」
  让 owner 明白阻断原因
- **placeholder 给三个真实场景例子**：让 owner 第一次开就知道这是
  "给 pet 留状态信号"而非"给 pet 发消息"
- **底部小字 hint**：列三个快捷键让 owner 一眼知道高频操作（重开 /
  默认提交 / 关）

## Key design decisions

- **不复用 ctxMenu 模式（坐标定位）**：transient_note 是 modal-level
  动作（影响 pet 整体行为），用 fixed 居中 modal 比 right-click ctxMenu
  视觉更重 — 提示 owner 这是「设置类」而非「单条消息处理」
- **不引入第二个 ⌘\` 行为**：当前未占用 ⌘\`；若以后想加（如「打开
  最近文件 picker」），priority 上 transient_note 仍是高频需求
- **不持久化 draft**：transient_note 本身是 in-memory N 分钟，draft
  跨 popover 重开记忆没意义；submit 后清空让下次重开是空白模板
- **复用既有 onSetTransientNote callback**：与 ChatBubble 右键
  「📝 用此话」同 channel — App.tsx 内 handleMiniSetTransientNote 已
  含 appendAssistant 反馈（"📝 已用此话设 transient_note（N 分钟）：
  「<preview>」"），不需另写反馈逻辑
- **不为单 popover 引 unit test runner**：行为是 setState + invoke
  既有命令；build pass + 手测足够：（1）ChatMini 可见时按 ⌘\` 弹起 →
  （2）输入文字 → 点 30m / 1h / 2h / 6h 任一 → 看 popover 关 + chat
  出现确认反馈 →（3）再按 ⌘\` 看 popover 关闭（toggle）→（4）outside-
  click / Esc 关

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 set_transient_note Tauri 命令
