# PanelMemory "📅 按时间排序" 全局 toggle

## 背景

PanelMemory 当前每个 category 的 items 按 yaml 文件顺序展示（pinned 抓到头部，剩余原序）。yaml 写入顺序对用户来说**没意义**（新增条目从末尾追加，但用户不知道某段是 yaml 何时插入）。日常翻 PanelMemory 最高频问题是 **"最近改了什么"**。

加一个全局 sort toggle，让 `rest`（非 pinned 段）按 `updated_at` 倒序，pinned 保持挂头。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### state + 持久化

```ts
const [sortByRecent, setSortByRecent] = useState<boolean>(() => {
  try {
    return window.localStorage.getItem("pet-memory-sort-recent") === "1";
  } catch { return false; }
});
```

toggle 时：

```ts
const toggleSortByRecent = () => {
  setSortByRecent((prev) => {
    const next = !prev;
    try {
      window.localStorage.setItem("pet-memory-sort-recent", next ? "1" : "0");
    } catch {}
    return next;
  });
};
```

#### toolbar 按钮

紧挨在 `⊟ 全折叠` 之后插：

```tsx
<button
  style={{ ...s.btn, ...(sortByRecent ? activeBtnStyle : {}) }}
  onClick={toggleSortByRecent}
  title="开 / 关：按 updated_at 倒序排列（pinned 仍在顶部）。关 → 走 yaml 文件原序。"
>
  📅 {sortByRecent ? "按时间" : "默认序"}
</button>
```

active 态用 tint-blue 染底色 + accent 字色，让 toggle 状态一眼可识别（与 PanelTasks 既有 active filter chip 同思路）。

#### 排序逻辑

在 line 2483-2489 那段：

```ts
// 当 sortByRecent 时，把 pinned 与 rest 各自按 updated_at 倒序。pinned
// 仍优先（用户主动钉 = 强信号），但段内也按时间排，"最近钉的"最先看到。
const cmpRecent = (a: MemoryItem, b: MemoryItem) =>
  (b.updated_at || "").localeCompare(a.updated_at || "");
const pinned: MemoryItem[] = [];
const rest: MemoryItem[] = [];
for (const it of scheduleFilteredItems) {
  if (pinnedKeys.has(`${catKey}::${it.title}`)) pinned.push(it);
  else rest.push(it);
}
if (sortByRecent) {
  pinned.sort(cmpRecent);
  rest.sort(cmpRecent);
}
const sortedItems = pinned.length > 0 ? [...pinned, ...rest] : (sortByRecent ? rest : scheduleFilteredItems);
```

注意：`scheduleFilteredItems` 在 sortByRecent=false + 无 pinned 时直接返回（保持现行 fallback 路径，避免无谓 sort 拷贝）。

## 不做

- 不做"每 category 独立 sort 模式"：toolbar 已经按钮多，再 per-section 加 chip 反而繁；用户日常需要的是"全局看最近"vs"yaml 默认"二态
- 不暴露"按 created_at 倒序"另一模式：updated_at 就是"最近活动"的最近代理；created_at 主要给历史考古，频次低
- 不动 schedule filter 的优先级：仍然先过 scheduleFilteredItems（每周提醒等），再 pinned/rest 拆分，最后 sort —— 三层独立，互不破坏

## 验收

- `npx tsc --noEmit` ✅
- 点 toolbar 「📅 默认序」 → 切到「📅 按时间」 → 列表内每段重排，最近改的在上
- 关闭面板再开 → toggle 状态保留
- pinned 项仍挂头，但顺序也按时间（无 pin 时单层时间排序）

## 完成

- [x] PanelMemory.tsx: state + persist + toolbar 按钮 + 排序逻辑
- [x] `npx tsc --noEmit` 通过
- [x] README 一行
- [x] 移到 docs/done/
