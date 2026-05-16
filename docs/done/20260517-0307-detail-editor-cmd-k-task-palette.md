# detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 背景

iter #183 加了 ↑/↓ 按钮 + iter #186 加了 ⌘[/⌘] 快捷键支持 detail 编辑器内顺序 prev/next 切换。但 owner 想跳到远离的 task（"上次写过的 X" / "P0 最重要那条" 等）必须连按 ⌘[ 多次 / 或退出编辑器走任务列表。

加 ⌘K 唤起 VSCode-style quick-find palette：input + fuzzy filter visibleTasks + Enter 跳。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 1. ⌘K 监听加进既有 ⌘[/⌘] effect

```ts
useEffect(() => {
  if (editingDetailTitle === null) return;
  const handler = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key === "[") { ... }
    else if (e.key === "]") { ... }
    else if (e.key === "k" || e.key === "K") {
      e.preventDefault();
      setTaskPaletteOpen(true);
      setPaletteQuery("");
      setPaletteSelectedIdx(0);
    }
  };
  ...
}, [editingDetailTitle, handleNavigateDetail]);
```

#### 2. 状态 + 通用切 task helper

```ts
const [taskPaletteOpen, setTaskPaletteOpen] = useState(false);
const [paletteQuery, setPaletteQuery] = useState("");
const [paletteSelectedIdx, setPaletteSelectedIdx] = useState(0);

const switchToTaskDetail = useCallback(async (targetTitle: string) => {
  // 复用 handleNavigateDetail 五步链路（dirty flush + detailMap 缓存 / IO
  // fallback + handleEnterEditDetail + setPendingTitleFocus），仅 target
  // 取法不同
  ...
}, [editingDetailTitle, editingDetailContent, visibleTasks, detailMap, handleEnterEditDetail]);
```

#### 3. Palette UI overlay

```tsx
{taskPaletteOpen && (() => {
  const filtered = q === "" ? visibleTasks.slice(0, 30) :
    visibleTasks.filter(t => t.title.toLowerCase().includes(q)).slice(0, 30);
  return (
    <div onMouseDown={... backdrop click 关 ...} style={fixed inset 0 + dark backdrop + paddingTop 10vh}>
      <div onMouseDown={stopPropagation} style={... 480 width card ...}>
        <input
          autoFocus
          value={paletteQuery}
          onChange={... reset idx to 0 ...}
          onKeyDown={handle Escape / ArrowDown / ArrowUp / Enter}
          placeholder="fuzzy 找 task （共 N）· ↑↓ 选 · Enter 切 · Esc 关"
        />
        <div maxHeight={360} overflow auto>
          {filtered.length === 0 ? "（无任务）" / "没有标题含「query」的任务" :
            filtered.map((t, i) => (
              <button
                onMouseEnter={() => setSelected(i)}
                onClick={() => { close + switchToTaskDetail(t.title); }}
                disabled={t.title === currentEditing}
                style={... active blue tint / current muted disabled / right side P priority chip ...}
              >
                {t.title}  P{t.priority}{isCurrent ? " · 当前" : ""}
              </button>
            ))}
        </div>
      </div>
    </div>
  );
})()}
```

#### 4. placeholder hint 补 ⌘K

两 textarea (edit / split) placeholder 末尾加 "/ ⌘K 跳到任意 task detail"。

## 关键设计

- **⌘K gate 在 editingDetailTitle 非空**：编辑模式才挂监听 + cleanup；non-editing 时 ⌘K 不抢用。
- **switchToTaskDetail extracted helper**：与 handleNavigateDetail 共享 dirty-flush + detailMap 缓存 + handleEnterEditDetail 五步链路，仅 target idx 取法不同。avoid copy-paste。
- **fuzzy = substring case-insensitive**：简单可靠；后续可升级 fuse.js 但当前用例（标题 ≤ 20 char + tasks 通常 < 50 条）足够。
- **filtered.slice(0, 30) cap**：避免极大队列时渲染 100+ 行卡 scroll；30 条已覆盖 owner "fuzzy 输几字就够定位"的场景。
- **空 query 也显前 30 条**：让 owner ⌘K 后直接 ↓↓↓ Enter 也能切（无需输入）；与 VSCode ⌘P 一致 UX。
- **mouse hover 同步 selectedIdx**：让鼠标用户也能"hover 切高亮 + Enter"。
- **当前 editingDetailTitle disabled**：防 owner 误切到当前（无效操作）；视觉 muted + 文字 "当前" hint。
- **backdrop click 关 + Esc 关 + 选中关**：三条退出路径，覆盖 mouse / keyboard / 完成 操作。
- **右侧 P{priority} chip**：让 owner 看 task 优先级，配合 fuzzy 找到目标。
- **autoFocus input**：开 palette 即可 type，不必额外 click。

## 不做

- **不显 due / tags / status 在 palette row**：信息密度噪音；owner 仅靠 title fuzzy 找。想看 task 元数据可关 palette 走 row hover preview / 展开。
- **不持久化 palette 历史 query / 上次 selection**：palette 是 ephemeral 入口；每次 fresh 开。
- **不限于 visibleTasks**：用既有 filter / sort 后的视图 —— 与 owner "当前看到的列表" 心智一致。想跨 filter 找 task 走 ⌘F 搜索清除 filter。
- **不写测试**：纯 React state + 既有 IPC（task_get_detail 已验证）+ 既有 handleEnterEditDetail；视觉验证（detail 编辑中按 ⌘K → palette 弹 + fuzzy + Enter 切）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~250 行（⌘K 监听 6 + state 5 + switchToTaskDetail helper 50 + palette overlay JSX 175 + placeholder hint 2 + 注释 12）。既有 ⌘[/⌘] 顺序导航 / handleNavigateDetail / handleEnterEditDetail / detailMap 缓存 / 编辑器 textareas 路径完全不动。

## TODO 状态

剩 0 条 —— TODO 池清空。下个 cron tick 进 auto-propose 分支。

## 后续

- ⌘K palette 加 fuse.js 模糊匹配（typo 容错 / 拼音 / 缩写）—— 但 visibleTasks 通常 < 100，简单 substring 已 acceptable。
- ⌘K 行 right-side 加 priority chip / due chip / "📌 钉" / "🔇 silent" 等 marker icon —— 让 fuzzy + visual filter 并存。
- palette 加 "📋 当前 / ↗ 仅 done / 🔴 仅逾期" filter row 让 owner 二级筛。
- 跨面板 ⌘K：在 PanelMemory / PanelChat 也加 ⌘K 入口（各自找 memory item / session）。
- ⌘K 后再按 ⌘K 切到 recently switched （MRU）顺序；让"在 N 个 task detail 间反复跳" 顺手。
