# PanelMemory item description # tag 自动补全 popover（iter #394）

## Background

iter #390 加 PanelTasks 搜索框 `#` tag 补全；本 iter 是对偶 — owner
在 PanelMemory 写 / 编辑 item description 时也想要 `#` 自动补全
免敲错 tag 名（敲错就漏 `/tags` / 桌面 chip filter 命中）。

完成 # tag autocomplete 三 surface 覆盖：PanelTasks 搜索框（#390）/
PanelMemory description（本 iter）/ detail.md @ task title（既有
atTrigger pattern）。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. state（~line 1053，紧贴 descTextareaRef）

```ts
const [descTagDismissedAt, setDescTagDismissedAt] = useState<number | null>(null);
const [descTagSelectedIdx, setDescTagSelectedIdx] = useState<number>(0);
const [descTextareaCursorPos, setDescTextareaCursorPos] = useState<number>(0);
```

完全镜像 iter #390 PanelTasks tagTrigger 系列。

#### 2. allTagFrequencies useMemo（global tag count）

```ts
const allTagFrequencies = useMemo(() => {
  const counts = new Map<string, number>();
  const re = /#[A-Za-z0-9_一-龥-]+/g;  // 与 line 6223 inline chip 同正则
  for (const cat of Object.values(index.categories)) {
    for (const it of cat.items) {
      const matches = it.description.match(re) ?? [];
      const seen = new Set<string>();
      for (const m of matches) {
        const t = m.slice(1).toLowerCase();
        if (seen.has(t)) continue;
        seen.add(t);
        counts.set(t, (counts.get(t) ?? 0) + 1);
      }
    }
  }
  return counts;
}, [index]);
```

跨所有 cat 聚合（不限当前 cat）— memory tag 是 cross-cat semantic
（同 #工作 出现在 user_profile / butler_tasks / general 不同 cat）。

#### 3. descTagTrigger memo

与 iter #390 tagTrigger 同 word-boundary 扫描。基于 `editingItem.description`
+ `descTextareaCursorPos`。

#### 4. descTagSuggestions memo

频次降序 + alphabetical tiebreak；query case-insensitive substring；
cap 8。

#### 5. acceptDescTagSuggestion + handleDescTagKeyDown

acceptDescTagSuggestion：`#query` 段（[hashPos, cursor)）替换为
`#<tag>`，光标落 token 末尾 + rAF refocus textarea + setSelectionRange。

handleDescTagKeyDown：popover active 时拦 ↑↓/Enter/Tab/Esc — 与
iter #390 handleTagKeyDown 同模板。

#### 6. 模态 textarea 包 position:relative wrapper + popover

```tsx
<div style={{ position: "relative" }}>
  <textarea ref={descTextareaRef}
    onChange={(e) => { setEditingItem(...); setDescTextareaCursorPos(e.target.selectionStart ?? 0); }}
    onSelect={...} onClick={...} onKeyUp={...}  // 4 入口同步 cursor pos
    onKeyDown={(e) => {
      if (handleDescTagKeyDown(e)) return;  // ↑↓/Enter/Tab/Esc 拦
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        // 既有 ⌘S 保存
      }
    }}
  />
  {descTagTrigger && descTagSuggestions.length > 0 && (
    <div style={{ position: "absolute", top: "100%", left: 0, right: 0, ... }}>
      header + suggestion list
    </div>
  )}
</div>
```

Popover 风格与 iter #390 一致（purple tint hover / count 右侧 mono
字体 / 顶部 hint 行）。

## Key design decisions

- **mirror iter #390 PanelTasks pattern**：完全复制 state /
  word-boundary parse / accept / dismiss 协议。owner 学一次三处通用。
- **cross-cat tag frequency**：tag 在 memory 内是 cross-cat 自由维度
  （不像 butler_tasks 内仅自段 tag）— 全 index 聚合 frequency 让
  owner 输 `#工` 时弹出真高频 tag 不论它出现在哪 cat。
- **复用既有正则 `#[A-Za-z0-9_一-龥-]+`**：与 inline chip 渲染（line
  6223）/ task_queue::parse_task_tags 边界一致 — owner 看到的 chip
  tag 就是补全候选 tag。
- **仅模态 textarea（new/edit）覆盖，不动 inline desc edit**：模态
  是主要新建路径；inline edit 是快速调整少数字符场景 — 加 # popover
  反而 UX 重。如未来反馈强烈再扩。
- **不持久化 dismissed state**：与 iter #390 同 — 按 hashPos sticky
  防"刚 Esc 又弹"，cursor 离开 word 后清。
- **searchCursorPos 多入口同步**：onChange/onSelect/onClick/onKeyUp
  四入口覆盖几乎所有 cursor 移动方式（React controlled textarea 无
  原生 cursor state）。
- **不为单 fn 引 unit test runner**：行为是 IO + state ops；build
  pass + 手测足够（打 `#` / `#工` / Esc 关 / 选条 accept 四场景）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.21s)
- 后端无改动 — 复用 index.categories 数据
