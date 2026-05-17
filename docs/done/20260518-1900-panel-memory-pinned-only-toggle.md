# PanelMemory 加全局「📌 仅 pinned」toggle（iter #491）

## Background

PanelMemory 既有 sort chip 行（📅 按时间 / 📏 按字数 / 🔀 按创建）让 owner
切换排序视角。pinned items 既有：

- 单 item 📌 chip 钉 / 取消钉
- pinned items 排序时挂头（与 sortByRecent / sortByCreated 等正交）
- 段内 fuzzy / silent / today-updated chip 过滤

但缺**「跨 cat 显所有 pinned items」**入口：owner 想一眼看 "我跨各 cat
共钉了哪些 N 条" — audit / spring-cleaning / "我钉的是否还相关" 场景。
要走目前路径只能逐 cat 滚 + 数 📌 行，效率低。

本 iter 加一个全局 toggle：**「📌 仅 pinned」** — true 时各 cat 仅显
本段 pinned 命中的 item，0 钉的 cat 整段隐藏。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. 新增 `pinnedOnly` state（line ~840）

```tsx
const [pinnedOnly, setPinnedOnly] = useState<boolean>(() => {
  try {
    return window.localStorage.getItem("pet-memory-pinned-only") === "1";
  } catch {
    return false;
  }
});
const togglePinnedOnly = () => {
  setPinnedOnly((prev) => {
    const next = !prev;
    try {
      window.localStorage.setItem(
        "pet-memory-pinned-only",
        next ? "1" : "0",
      );
    } catch {}
    return next;
  });
};
```

与 sortByRecent / sortByCharCount / sortByCreated 同 localStorage 持久化
pattern；key `pet-memory-pinned-only`。

#### 2. 在 cat loop 起始处加跳过逻辑

```tsx
if (pinnedOnly) {
  const hasAnyPinned = cat.items.some((it) =>
    pinnedKeys.has(`${catKey}::${it.title}`),
  );
  if (!hasAnyPinned) return null;
}
```

0 钉的 cat 整段隐藏（不只 body 空），让 "总览" 视图更紧凑。

#### 3. 在 `scheduleFilteredItems` pool 起始处加 filter

```tsx
let pool = cat.items;
if (pinnedOnly) {
  pool = pool.filter((it) =>
    pinnedKeys.has(`${catKey}::${it.title}`),
  );
}
// ... fuzzy / today / silent / today-updated / schedule-kind filter 接续
```

与下游 filter AND 叠加；与 sort 正交。

#### 4. sort chip 行末加 📌 toggle chip

```tsx
<button
  style={pinnedOnly ? { ...s.btn, background: tint-yellow, ... } : s.btn}
  onClick={togglePinnedOnly}
  title={`...（当前共 ${pinnedKeys.size} 钉）...`}
>
  📌 {pinnedOnly ? `仅钉(${pinnedKeys.size})` : `钉 -`}
</button>
```

底色染 tint-yellow（与 📌 emoji 黄色语义一致），与既有 sortBy* chip 的
tint-blue 区分 — sort 是 "排序视角"，pinned-only 是 "范围视角"。

标签括号显当前 `pinnedKeys.size` —— owner 切换前能预估视图密度（"我才
钉 3 条" vs "我钉了 47 条"）。

## Key design decisions

- **整段隐藏 0 钉 cat**（return null at outer loop）而非 "空段保留 header"：
  "总览：我钉了哪些" UX 需要 "无关 cat 直接消失"，而不是 N 个空段堆叠
  视觉噪音。pinned-only 是 "范围视角" 切换，本质就该收窄 cat list
- **与 sortBy* 正交**（不互斥）：pinnedOnly 是"过滤"，sortBy* 是"排序"。
  二者纬度不同，应可独立切换（e.g. "仅钉 + 按字数倒序" 看 "我钉的中
  哪些 content 最重"）
- **与 fuzzy / silent / today-updated AND 叠加**：pinnedOnly 收 pool 后
  剩余 filter 继续作用 — e.g. "仅钉 + silent" = "我钉的静默 task 有
  哪些"
- **localStorage 持久**：与 sortByRecent / sortByCreated 等同模式 —
  pinned-only 是阅读偏好，下次打开 panel 保留
- **tint-yellow active 态**：📌 emoji 黄色 visual cue + 与 sortBy* 的
  tint-blue 区分 "范围 vs 排序" 二类 toggle
- **括号显总钉数**：owner 切换前能预估 "切过去看到多少" — 与 PanelTasks
  bulk action bar 标题显计数同 UX 哲学
- **不强制清空其它 filter**：pinnedOnly 切到 ON 时既有 fuzzy / silent
  chip 状态保留，按既定 AND 叠加 — 不引 "切到 pinned-only 我之前的
  filter 没了" 的隐式状态破坏
- **不写 unit test**：纯 React state + filter / sort 流；逻辑 trivial
  （既有 sortBy* / silent chip 同 filter 路径 production 验证）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
- 后端无改动 — 纯前端 toggle
- 手测：PanelMemory 打开 → 多 cat 各钉几条 → 点 sort 行末「📌 钉 -」chip
  → 变 tint-yellow 「📌 仅钉(N)」→ 视图收窄到全 pinned items（0 钉 cat
  整段隐藏）→ 与既有 sortBy* chip 叠加切换看不同排序的 "我钉的" → 再
  点 chip 退出仅钉视图 → 刷新 panel 偏好保留

## Future iters (out of scope)

- 全 pinned items 跨 cat **统一时序排列**（无 cat 分段）— 当前仍保留
  cat header 显 "这条钉的来自哪段"，要"完全无段"切到第二级 view 即可
- 「💾 钉清单 → pinned.md 」一键 export — 可走既有 PanelMemory `📤 .md`
  chip family 模板加 pinned-only mode
