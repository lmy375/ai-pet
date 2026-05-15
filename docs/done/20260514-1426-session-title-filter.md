# PanelChat 会话下拉标题搜索框

## 背景

TODO（本轮 auto-proposed）：

> PanelChat 会话下拉加标题搜索框：30+ session 时按标题子串过滤，与既有 today / images / tasks chip 组合生效。

会话下拉已有 chip filter（📅 今日 / 📷 含图片 / 📋 含派单），但全凭 chip 找不到"那条关于 Downloads 的会话"——chip 是 semantic 过滤，title 是 lexical 过滤，二者互补。30+ session 用户尤其感受到痛点。加一行 free-text title 输入框是最低代价的查找入口。

## 改动（frontend only）

### `src/components/panel/PanelChat.tsx`

**1. state + 自动清空**

```ts
const [sessionTitleQuery, setSessionTitleQuery] = useState("");
useEffect(() => {
  if (!showSessionList) setSessionTitleQuery("");
}, [showSessionList]);
```

下拉关闭时 query 清掉 —— 下次开 dropdown 是干净的搜索体验。

**2. 输入框 render**

```tsx
{sessionList.length > 5 && (
  <div style={{ padding: "6px 12px", borderBottom: "...", display: "flex", gap: 6 }}>
    <input
      type="text"
      value={sessionTitleQuery}
      onChange={(e) => setSessionTitleQuery(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          if (sessionTitleQuery) setSessionTitleQuery("");
          else setShowSessionList(false);
        }
      }}
      placeholder="按标题筛选…（Esc 清空 / 关下拉）"
      style={{...}}
    />
    {sessionTitleQuery && (
      <button onClick={() => setSessionTitleQuery("")}>✕</button>
    )}
  </div>
)}
```

- `sessionList.length > 5` 门限：5 条以内全部都在视野里，加输入框反而 visual noise；> 5 自然出现。
- `Esc` 两段语义：query 非空 → 清 query（保留下拉开着）；query 空 → 关下拉。与 chat search input 同节奏。
- 右侧 `✕` 按钮在 query 非空时浮出 —— 鼠标用户单击清；键盘党仍可 Esc。

**3. 过滤管线扩展**

```ts
const chipFiltered = sessionFilter !== null && filterSessionIds !== null
  ? reversed.filter(s => filterSessionIds.has(s.id))
  : reversed;
const titleQuery = sessionTitleQuery.trim().toLowerCase();
const filtered = titleQuery
  ? chipFiltered.filter(s => s.title.toLowerCase().includes(titleQuery))
  : chipFiltered;
```

**AND 组合**：chip 先过滤（语义维度），title 再过滤（lexical 维度）。让"📋 含派单 + 标题含 Downloads" 自然成立。空 query 直通让 chip-only 路径性能等同既有。

**4. 三态 empty message**

```ts
if (filtered.length === 0 && (sessionFilter !== null || titleQuery)) {
  const reason =
    sessionFilter !== null && titleQuery
      ? `chip 「${chipLabel}」与标题「${titleQuery}」组合无命中`
      : sessionFilter !== null
        ? "chip 过滤无命中"
        : `没有标题含「${titleQuery}」的会话`;
  const hint =
    sessionFilter !== null && titleQuery
      ? "改 chip 或清标题再试"
      : sessionFilter !== null
        ? `点 ${chipLabel} 关闭过滤`
        : "Esc 清空筛选";
  return <EmptyState icon="🔍" title={reason} hint={hint} compact />;
}
```

让"为什么我没看到任何 session"的反馈足够具体 —— 区分 chip 单独命中 0、title 单独命中 0、两者组合命中 0 三种语境，hint 引导用户走对应的"打开 / 清"操作。

## 不做

- **不 fuzzy / 子序列匹配**。子串足够：用户记得 "Downloads" 5 个字时不会期望 "Dwnlds" 也命中。fuzzy 加在搜索算法基线复杂度 + 排序歧义，本场景 < 100 session 数量级，子串实测 < 1ms。
- **不限制只筛 unpinned**。pinned 段也会跟 title 过滤 —— 用户清 query 时 pinned 自然重新挂头，filter active 时 pinned 也得跟语义走（"被钉但与查询无关"的 pinned 不该浮在结果上)。
- **不持久化 query 到 localStorage**。本来就是"session 内临时查找"工具，跨重启清掉合理。
- **不暴露设置开关**。`> 5 条阈值显输入框`是经验值；5 以内的用户压根不需要这个 UI，触发感是"我有了不少 session 了"的自然过渡。
- **不动跨会话 message 内容搜索 `/search`**。两个是不同入口：title 找 session 容器，content `/search` 找特定内容片段。互不干扰。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~80 行（state 8 + input render 50 + 过滤管线 + empty 三态 25）；既有 chip filter / pinned 优先 / inline rename / right-click context menu 全部不动。

## 后续

- 顶部 tab bar（"⋯ +N"按钮）旁边也加快捷输入：let user 直接搜不必先开下拉。
- 标题搜索与跨会话 message search `/search` 的入口合并 / 切换 toggle —— 当前两个独立。
