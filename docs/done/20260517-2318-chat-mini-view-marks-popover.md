# ChatMini「📌 view marks」popover + duplicate PanelTasks task pivot（iter #417）

## Background

iter #412 加 ChatMini 选区 toolbar 📌 标记按钮，写 sessionId::sel-<ms>
key 到 pet-chat-marked-messages localStorage 但无 view UI — owner
标过就只能在 console.info 看不到回头审视。

本 iter 补 view-marks popover + 同时延伸 iter #412 写路径让 mark
text body 也持久化到 sibling key（之前只存 timestamp）。

同时丢 TODO "PanelTasks task 右键菜单加「📑 复制为新 task」" —
该功能已经存在为 🪞 克隆任务 button（line 16041-16064，调
task_clone 后端：strip_for_clone 剥终态 / snooze marker + 复
detail.md + (副本 N) 自增 unique-suffix）。TODO 描述的"-copy-N"
suffix 是 PanelMemory iter #405 风格，与已有 task 风「(副本 N)」
仅 cosmetic 差别；不再加重复按钮，丢线说明。

## Changes

### `src/components/ChatMini.tsx`

#### 1. 扩 iter #412 写路径加 sibling text key

```ts
// 新 sibling key: pet-chatmini-mark-texts
// Schema: Record<markKey, text>
const TEXTS_KEY = "pet-chatmini-mark-texts";
let texts: Record<string, string> = ...;
texts[markKey] = text.length > 120
  ? text.slice(0, 120) + "…"
  : text;
window.localStorage.setItem(TEXTS_KEY, JSON.stringify(texts));
```

120 字 cap：让 popover 行高紧凑；超长选段尾省略号。

write 后 `setMarksRefreshTrigger(v => v + 1)` 让 view popover state
即时刷新（无需等 popover 重开）。

#### 2. State + refresh logic

```ts
const [marksPopoverOpen, setMarksPopoverOpen] = useState(false);
const [marksRefreshTrigger, setMarksRefreshTrigger] = useState(0);
const [marksList, setMarksList] = useState<{key, ts, text}[]>([]);
const [marksCount, setMarksCount] = useState(0);

const refreshMarks = useCallback(async () => {
  const idx = await invoke<{active_id: string}>("list_sessions");
  const sid = idx.active_id?.trim();
  if (!sid) { setMarksList([]); setMarksCount(0); return; }
  // 读两个 key：pet-chat-marked-messages（filter sel-*）+ pet-chatmini-mark-texts
  // session prefix filter `${sid}::sel-`
  // 按 ts desc 排（最新在前）
  setMarksList(out); setMarksCount(out.length);
}, []);

useEffect(() => { void refreshMarks(); }, [refreshMarks, marksPopoverOpen, marksRefreshTrigger]);
```

旧 mark (iter #412 之前) 没 text body → fallback "（无文本快照
— iter #412 之前的旧 mark）"。

#### 3. deleteMark callback

从两个 localStorage key 同步删 — markedMessages 和 texts 走相同
key，单条 delete 一并清理。trigger refresh。

#### 4. 📌 N chip button（top-right chip row）

仅 marksCount > 0 时浮起。位置：onOpenPanel 132px / no-panel 104px
（紧贴 💾 export 按钮，与既有 28px 间距节奏一致）。yellow tint chip
+ count badge label。click → toggle popover。

#### 5. fixed modal popover

- backdrop 半透关 outside-click
- header：📌 本会话标记 (N) + ✕ 关
- body：empty 兜底「选中文字 → 工具栏 📌 标记可加入」/ 列表每条
  `[MM-DD HH:MM] text snippet 🗑`
- 自动按 ts desc 排，max-height 70vh 防溢出长列表
- title attr 显完整 toLocaleString 时间戳

### `docs/TODO.md`

drop "PanelTasks task 右键菜单加「📑 复制为新 task」" 一行 —
该功能由已存在的 🪞 克隆任务 button（task_clone Tauri 后端）满
足；TODO 文案描述的"-copy-N"suffix 是 cosmetic 偏好，不重复加。

## Key design decisions

- **sibling key 而非扩 schema**：PanelChat read 路径只取 number
  value（idx-based + sel-* 同 schema），扩 value 形态会破坏既有 read。
  双 key 让各自 schema 独立演化
- **120 字 cap**：popover 行紧凑 + 选段过长滚屏负担；超长部分省略号
  暗示「完整内容请用 📋 复制 / 看原 bubble」
- **本会话 scope**：只渲 `${sid}::sel-` 前缀的 marks — 跨 session
  marks 不混入避免视觉噪音。owner 切 session 时 chip count 自动
  归零
- **refreshTrigger 让 write → view 即时**：而非依赖 storage event
  （跨 window 才 fire）；同 window 内 state 同步靠手动 trigger
- **不实现批量删除 / 导出 popover**：MVP — 单条 🗑 已覆盖核心操作；
  批量需求出现再扩
- **不为单 popover 引 unit test**：纯 localStorage read/write +
  setState；build pass + 手测足够（选段 📌 → chip 浮 → click chip
  弹 popover 见 list → 🗑 删一条 → list 减少 → chip 0 时消失）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.45s)
- 后端无改动 — 纯前端 localStorage + invoke("list_sessions")
