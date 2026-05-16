# PanelMemory 顶 "🌱 今日新增 N" chip click 弹 drill-down modal

## 背景

PanelMemory 顶 chip 行已有 "🌱 今日新增 N" 计数（created_at 以今日开头）但只是 read-only 数字。owner 看到"今天新增 8 条"想知道"具体是哪 8 条" —— 必须滚每个类目找。

加 chip click → drill-down modal 列按类目分段的今日新增 items 清单，让 owner 一眼看 "今天 / 宠物 / 我自己写了什么"。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### 1. 新 `todayNewDrillOpen: boolean` state

```ts
const [todayNewDrillOpen, setTodayNewDrillOpen] = useState(false);
```

#### 2. 顶 chip 从 `<span>` 改 `<button>`

```tsx
<button
  onClick={() => setTodayNewDrillOpen(true)}
  style={{
    color: "tint-green-fg",
    background: "transparent",
    border: "none",
    cursor: "pointer",
    textDecoration: "underline",
    textDecorationStyle: "dotted",  // 视觉提示可点
  }}
  title="...点击 drill-down 看具体清单（按类目分组）"
>
  🌱 今日新增 {todayNewCount}
</button>
```

#### 3. 新 drill-down Modal

```tsx
<Modal open={todayNewDrillOpen} onClose={() => setTodayNewDrillOpen(false)} maxWidth={440}>
  <div>
    <div>🌱 今日新增 N 条记忆</div>
    <div>按 created_at 起始 = 今日（本机时区）筛...</div>
    {(() => {
      const today = new Date().toLocaleDateString("sv-SE");
      const sections = [];
      for (const catKey of CATEGORY_ORDER) {
        const cat = index.categories[catKey];
        const todayItems = cat.items.filter(it => it.created_at?.startsWith(today));
        if (todayItems.length > 0) sections.push({cat, label, items});
      }
      if (sections.length === 0) return <div>（未找到今日新增...）</div>;
      return sections.map(sec => (
        <div>
          <div>{label}（{N}）</div>
          <ul>
            {sec.items.map(it => (
              <li>
                <span>{HH:MM}</span> {it.title}
              </li>
            ))}
          </ul>
        </div>
      ));
    })()}
    <button onClick={close}>关闭</button>
  </div>
</Modal>
```

按 CATEGORY_ORDER 排（butler_tasks / todo / ai_insights / user_profile / task_archive / general）—— 活跃类目压在前。每条 item 显 "HH:MM title"，时间作 ambient 锚点。

## 关键设计

- **read-only drill-down**：modal 内仅展示清单，不提供 inline 编辑入口。想编辑回类目段双击 title（onboarding hint iter #201）。简单粗暴减少 modal 复杂度。
- **toLocaleDateString("sv-SE") 取本地日期**：与既有 todayNewCount useMemo 同算法 —— 与 created_at ISO 前 10 字符兼容，不被 UTC 折日。
- **CATEGORY_ORDER 排序**：与主面板 section 排序一致 —— owner 心智模型不需切换。
- **HH:MM 显时间**：每条 item created_at 取 11-16 字符 = "HH:MM"。让 owner 看 "今早写了什么 vs 晚上写了什么"。
- **空清单兜底 italic muted**：边界 case（created_at 字段非标准 / 计数错位）给 hint 而不空 modal。
- **chip 用 dotted underline 提示可点**：透明 bg + 下划线 dotted —— 与 PanelTasks 顶 tone strip 等 clickable chip 视觉语言一致。
- **maxHeight 360 + overflow auto**：> N 条 item 时滚动；不撑爆 modal。

## 不做

- **不写实际编辑入口**：drill-down 是浏览视图；想改回类目段。
- **不显 description 摘要**：避免列表过长；owner 想看 description 双击 item 进编辑模式（既有路径）。
- **不绑 ⌘+click item 跳行**：scope creep；既有 setPendingTitleFocus 可对接但本 iter 只读。
- **不写测试**：纯 React modal + filter；视觉验证（含今日新增 item 时 chip click → modal 应列分段清单）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~140 行（state 4 + chip 改 button 15 + Modal 130 + 注释）。既有 todayNewCount memo / chip 视觉 / Modal pattern / CATEGORY_ORDER / loadIndex 路径完全不动。

## TODO 状态

剩 1 条留池：
- PanelSettings 顶 search input

## 后续

- "🌱 click → modal item click 跳行" —— 复用 setPendingTitleFocus pipeline；item click 关 modal + 滚到 cat 段 + 高亮 item。
- 今日新增同时间线视图：横向 24h timeline 显 item 落点 + click 跳。
- "🌱 按天" drilldown 支持选历史日期 —— 让 owner 回看 "上周三宠物写了什么"。
