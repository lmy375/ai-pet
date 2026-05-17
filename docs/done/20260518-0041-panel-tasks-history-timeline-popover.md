# PanelTasks 任务行右键「📊 看 history timeline」popover（iter #432）

## Background

PanelTasks task ctxMenu 既有「🪞 克隆任务」/「📋 复制 detail.md」
等动作，但「快速看这条 task 的 butler_history 事件清单」需先点
「📂 展开详情」全展开 detail panel 再滚到底找「事件时间线」段 —
多步开销。

本 iter 加 ctxMenu「📊 看 history timeline」item — click 直接弹
fixed-center modal 显该 task 的事件清单（reuse 既有
task_get_detail.history + detailMap 缓存）。owner 快速 audit 入
口；与 TG /timeline 命令同 SoT（同 backend 数据，仅前端 UI 不同）。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `historyTimelinePopover` state（紧贴 detailMap 之后）

```ts
const [historyTimelinePopover, setHistoryTimelinePopover] = useState<
  | { title: string; events: TaskHistoryEvent[] | null; ioError: boolean }
  | null
>(null);
```

null = 关；events=null = loading（owner 见反馈）；events=[] +
ioError=true = IO 失败但 popover 仍开显警告。

**位置关键**：必须在 detailMap 声明之后；之前一次放到 taskCtxMenu
旁边触发 TDZ — useEffect / useCallback 引用 detailMap / state 但
两者声明顺序倒挂（与 iter #427 PanelDebug muteRemainingMins 同 fix
模式 —— React 渲染按 top-down 求值，引用必须 lexical-after 声明
点）。

#### 2. `openHistoryTimelinePopover` callback

先 setState 进 loading state（让 owner 见反馈）；缓存命中即用；
否则 invoke task_get_detail 拉新数据 + 塞 detailMap 让后续动作复用；
**race guard**：fetch 期间 owner 可能切到别的 task / 关 popover —
setState 时 `cur && cur.title === title` 检查防 stale 数据覆盖
新视图。

#### 3. Esc 关 useEffect

mousedown outside-click 已由背景 div 处理；Esc 单独全局监听 —
仅 popover 开时挂。

#### 4. ctxMenu item（紧贴 🪞 克隆任务 之后）

```tsx
{t && (
  <button onClick={() => {
    setTaskCtxMenu(null);
    void openHistoryTimelinePopover(m.title);
  }}>
    📊 看 history timeline
  </button>
)}
```

gate by `t` — 任务存在才显（与既有克隆按钮同语义）。setTaskCtxMenu(null)
立即关 ctxMenu 让 owner 看到 popover 即将打开。

#### 5. fixed-center modal popover 渲染

```tsx
{historyTimelinePopover && (
  <div onMouseDown={outsideClickHandler} style={overlay}>
    <div onMouseDown={stopProp} style={panel}>
      <header>📊 「<title>」事件时间线 · 共 N 条 · ✕ 关</header>
      {ioError && <warning>⚠ 读 log 失败</warning>}
      {events === null ? "读取中…"
       : events.length === 0 ? "（无事件 / 因读失败）"
       : events.map(ev => (
           <row>
             <span>{emoji}</span>          // 📝 create / ✏️ update / 🗑 delete
             <span monospace>{tsShort}</span>  // YYYY-MM-DD HH:MM
             <span>{ev.action}{ev.snippet ? ` :: ${ev.snippet}` : ""}</span>
           </row>
         ))
      }
    </div>
  </div>
)}
```

设计要点：
- **events sorted newest-first**：与既有 task_get_detail.history 返
  回顺序一致（backend 已 sort）；popover 也是新事件在顶
- **emoji map create=📝 / update=✏️ / delete=🗑**：与 TG /timeline
  formatter 同；cross-surface 视觉一致
- **ts 截前 16 字 + T→空格**：与既有 expand 段「事件时间线」格式
  一致（YYYY-MM-DD HH:MM）
- **70vh max-height + overflow auto**：长 history（数百行）滚屏；
  modal 不撑爆视口
- **action + snippet 用 `::` 分隔**：与既有 PanelTasks expand 段格
  式一致 + butler_history.log 行内格式 `<action> <title> :: <snippet>`
  同惯
- **半透 backdrop + outside-click 关**：标准 modal pattern；与 ChatMini
  transient_note popover (iter #404) / view marks popover (iter #417)
  视觉一致

## Key design decisions

- **复用 task_get_detail 不另开 backend**：既有 backend 已暴露
  history 数据 + IO 错状态；新增 backend command 是重复劳动
- **race guard via title 比较**：异步 fetch 期间 owner 切到别的
  task → 旧 fetch resolve 时 popover 已切走，setState 会污染新视
  图。`cur && cur.title === title` 保证只更新匹配的 popover
- **loading state 立即开 popover**：相比 fetch-then-open 流程，让
  owner 立刻看到反馈（"读取中…"）— UX 更连贯
- **不引时间线 emoji（如 🕰️ / 📅）vs 仅 📊**：📊 与既有「📊 30
  天 sparkline」chip 同 emoji — 跨入口同语义（事件分布）
- **不为 popover 引 unit test**：纯派生 + setState；行为是 invoke
  既有 + Modal 渲染；build pass + 手测足够（右键 task → 看「📊 看
  history timeline」item → click → 看 popover 弹起 + 列事件 →
  Esc / outside-click / ✕ 关）

## Verification

- `npx tsc --noEmit`（frontend）— clean（修复 2 处 TDZ — 把
  state 声明从 taskCtxMenu 旁挪到 detailMap 旁；把 callback +
  useEffect 跟随移动）
- `npx vite build`（frontend）— clean (1.40s)
- 后端无改动 — 复用 task_get_detail Tauri 命令
