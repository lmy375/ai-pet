# PanelMemory 概览 "🌱 今日新增 N" chip

## 背景

PanelMemory 顶部概览栏已经有 `📚 N 条记忆` 和 `💾 N bytes (M 个文件)` 两个 chip，但没有"今日活动"维度 —— 用户没法一眼看到"宠物今天新长出多少条记忆"。配合上轮的 `📅 按时间排序` toggle，这种"今日新鲜度"信号更完整。

## 改动

`src/components/panel/PanelMemory.tsx`：

### 派生 `todayNewCount`

memo 化与 `totalMemoryCount` 同模式：

```ts
const todayNewCount = useMemo(() => {
  if (!index) return 0;
  // 本地日期前缀（YYYY-MM-DD）。toLocaleDateString("sv-SE") 给 ISO 但走
  // 本地时区 —— 与 created_at 写盘端格式（含 +08:00 offset）的前 10 字符
  // 兼容。
  const today = new Date().toLocaleDateString("sv-SE");
  let n = 0;
  for (const cat of Object.values(index.categories)) {
    for (const it of cat.items) {
      if (it.created_at && it.created_at.startsWith(today)) n += 1;
    }
  }
  return n;
}, [index]);
```

### 渲染

紧跟 `📚 N 条记忆` 后插一个 chip：

```tsx
{todayNewCount > 0 && (
  <span
    style={{ color: "var(--pet-tint-green-fg)" }}
    title={`今天新增 ${todayNewCount} 条记忆（created_at 以今天日期开头）`}
  >
    🌱 今日新增 {todayNewCount}
  </span>
)}
```

只在 `> 0` 时渲染：保持平静日里的概览整洁。

## 不做

- 不显"昨日新增 / 本周新增"对比：每加一个维度都让 chip 行更挤；今日是最高价值信号
- 不点击展开（"点这里看今日新增的标题清单"）：那是 search 框里输 `:today` 这种过滤的活；本轮不动 search DSL
- 不分 category 计今日（"butler_tasks 新增 X · user_profile 新增 Y"）：噪音；用户切到具体 category section header 已能看到该段最近更新

## 验收

- `npx tsc --noEmit` ✅
- 切「记忆」tab → 顶部若今日确有新增 → 多一个绿色 "🌱 今日新增 N"
- 空闲日（无新增）→ chip 不显，无 layout 抖动

## 完成

- [x] PanelMemory.tsx: useMemo 派生 + chip 渲染
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
