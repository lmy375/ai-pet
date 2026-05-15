# PanelChat "今日会话" 计数器 → 可点击 session filter

## 背景

PanelChat 头部 chip "📅 N · M" 显示今日活跃会话数 + 累计消息数，但 **cursor: default**，纯信息标牌。session 下拉里有 "📷 含图片" / "📋 含派单" 两条 filter chip 但**没有 "📅 今日"**。

用户想"只看今日"时还得手动滚 session list 用日期辨别。把"今日计数器"做成可点击 + 加一条 "📅 今日" filter chip = 一致的体感。

## 改动

`src/components/panel/PanelChat.tsx`：

### `SessionFilter` 加 `"today"`

```ts
type SessionFilter = null | "images" | "tasks" | "today";
```

### `toggleSessionFilter` 走本地 short-circuit

`"today"` 不调后端，从 `sessionList` 派生：

```ts
const toggleSessionFilter = useCallback(
  async (next: Exclude<SessionFilter, null>) => {
    if (sessionFilter === next) { ... 关 filter ... return; }
    setSessionFilter(next);
    if (next === "today") {
      // 本地 derive，避开 invoke 往返
      const todayPrefix = new Date().toLocaleDateString("sv-SE");
      const ids = new Set(
        sessionList
          .filter((s) => s.updated_at.startsWith(todayPrefix))
          .map((s) => s.id),
      );
      setFilterSessionIds(ids);
      return;
    }
    // 其它 filter 仍走 backend（保留原 logic）
    setFilterSessionIds(null);
    setFilterLoading(true);
    try { ... } finally { setFilterLoading(false); }
  },
  [sessionFilter, sessionList],
);
```

### Filter strip 加新 chip

`{([{kind:"images",...},{kind:"tasks",...},{kind:"today",...}])` 第三条：

```ts
{ kind: "today" as const, label: "📅 今日", desc: "只显今日活跃过的会话（updated_at 在今天）" },
```

### 顶部"今日计数器" chip 变可点击

`cursor: "default"` → `"pointer"`，onClick：

```ts
() => {
  void toggleSessionFilter("today");
  setShowSessionList(true);
}
```

可点击后样式联动：`sessionFilter === "today"` 时 chip 染 tint-blue 突出 active。

## 不做

- 不在 today filter 上加 loading 态：本地 derive 同步无延迟
- 不复用 today's prefix 与 task 相关 todayPrefix：作用域不同，inline 计算两行无可观察影响
- 不加测试：纯前端，无 vitest

## 验收

- `npx tsc --noEmit` ✅
- 点顶部 "📅 N · M" → session 下拉打开 + "📅 今日" chip active + 列表只显今日 sessions
- 再点关 filter；点 "📷 含图片" 切到其它 filter 也正常互斥

## 完成

- [x] SessionFilter 类型加 "today"
- [x] toggleSessionFilter 本地 short-circuit
- [x] filter strip 加 chip
- [x] 顶部计数器 chip 可点击 + active 染色
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
