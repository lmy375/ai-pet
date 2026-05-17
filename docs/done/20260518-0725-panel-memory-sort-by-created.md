# PanelMemory 加「🔀 按 created 排」toggle（iter #459）

## Background

PanelMemory 已有三个排序模式 toggles：
- 📅 按时间（`sortByRecent` — updated_at 倒序）
- 📏 按字数（`sortByCharCount`）
- ⏰ next-fire（`sortBulterByNextFire`，butler_tasks 专属）

但缺一个常用视角：**按 created_at 倒序**。

owner 想 audit「我什么顺序加进来的」/「最近新建了哪些」时：
- 默认 yaml 序：受 pinned / 编辑动作扰动，看不出添加时序
- updated_at 倒序：被「最近 hover 改过」干扰 — 一条老 item 改一个字
  就跳到顶
- created_at 倒序：纯添加时序，pet / owner 添加的顺序原样

本 iter 加 `🔀 按创建` toggle — 与既有三个排序 toggles 共生。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. `sortByCreated` state + `toggleSortByCreated` handler

紧贴 `sortByCharCount` 之后，复用既有 localStorage 持久化模板 + state
+ toggle pattern：

```ts
const [sortByCreated, setSortByCreated] = useState<boolean>(() => {
  try {
    return window.localStorage.getItem("pet-memory-sort-created") === "1";
  } catch {
    return false;
  }
});
const toggleSortByCreated = () => { ... };
```

localStorage key `pet-memory-sort-created`（与 `pet-memory-sort-recent` /
`pet-memory-sort-charcount` / `pet-butler-sort-next-fire` 并列）。

#### 2. Sort 级联 priority cascade

紧贴既有 `else if (sortByRecent)` 之后插：

```ts
} else if (sortByCreated) {
  // 按 created_at 倒序（ISO 字典序 = 时间序）
  const cmpCreated = (a: MemoryItem, b: MemoryItem) =>
    (b.created_at || "").localeCompare(a.created_at || "");
  pinned.sort(cmpCreated);
  rest.sort(cmpCreated);
}
```

四态互斥优先级：**next-fire > 字数 > recent > created > yaml 默认序**。
pinned 仍挂头不变。

#### 3. 工具栏 toggle 按钮

紧贴 📏 字数 toggle 之后插：

```tsx
<button
  style={sortByCreated ? blueTint : s.btn}
  onClick={toggleSortByCreated}
  title={sortByCreated
    ? "现按 created_at 倒序。点击切回 yaml 文件原序。pinned 仍挂头。"
    : "切到按 created_at 倒序（最近创建在上）— 「我什么顺序加的」audit。与 📅 按时间（updated）互补。pinned 仍挂头。"}
>
  🔀 {sortByCreated ? "按创建" : "创建 -"}
</button>
```

## Key design decisions

- **与 sortByRecent 互补而非替代**：created_at 看"添加时序"，updated_at
  看"改动时序"。pet 后台 consolidate / owner edit 一条老 item 会让
  updated_at 跳到现在 — 那条用 sortByRecent 浮顶就掩盖了真正"最近添加"
  的条目；用 sortByCreated 还原纯 add-time 视角
- **四态互斥，priority cascade**：next-fire (butler 专属) > 字数 > recent
  > created > yaml 默认。next-fire 是「未来视角」最强；字数是「内容
  重量审计」最具体；recent / created 是「时间视角」两轴；默认序是
  fallback。同时开多个 toggle 时 cascade 让 owner 不必关其它就生效
- **`(b.created_at || "").localeCompare(a.created_at || "")`**：ISO 字
  典序 = 时间序（已 production 验证 — sortByRecent 同算法）。空 created_at
  / undefined → `""` 排末（与 sortByRecent 同 fallback）
- **不写 unit test**：纯 sort comparator + localStorage toggle；逻辑
  trivial（既有 sortByRecent 同算法 production 验证）；`tsc` + `vite
  build` clean 即够。GOAL.md "meaningful tests only" 规则下不引装饰性
  测试
- **toolbar 全局位置而非 per-section**：与既有 📅 / 📏 / ⏰ toggle 同
  全局 toolbar，let 一处 toggle 影响所有 cat — 6 toggle / cat × N cat
  视觉爆炸；全局 toggle + 按 cat 各自应用是更稳的 UX
- **🔀 emoji**：与 sortByRecent (📅)/ sortByCharCount (📏)/ sortBulter
  ByNextFire (⏰) 视觉区分 — 🔀 是「shuffle / re-order」语义，强调
  「重新排序」动作；与 owner 心智「换个排序」对齐

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 纯前端 UI toggle + sort branch
- 手测：PanelMemory toolbar 看「🔀 创建 -」灰 toggle → 点击 → 高亮变
  「🔀 按创建」蓝底 → 看各 cat rest 段重排为 created_at 倒序（最新
  创建的 item 在前）；同时与 📅 按时间 toggle 互测确认 priority cascade
