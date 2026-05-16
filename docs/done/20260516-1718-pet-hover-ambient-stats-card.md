# pet 主区 hover 3s 浮 ambient 四段统计微卡片

## 背景

TODO 上一条："桌面 pet 主区 hover 自身（不点）3s 浮 ambient 「今日 / 本周 / 累计」三段统计微卡片：聚合既有 🐾 / ✦ chip 数据让 owner 一瞥全景。"

桌面 pet 顶部已有 🐾 N（今日主动开口）和 ✦ N（陪伴天数）两个 chip。owner 想知道"本周 / 累计宠物来找我几次" 时只能开 Panel → 人格 tab。

加一个"在 pet 上停留 3s 自然浮出 ambient 卡片"，让 owner 不必离开桌面视图即可一瞥宏观陪伴。

## 改动

### `src/App.tsx`

#### State + hover handlers

```ts
const [ambientStats, setAmbientStats] = useState<
  { today: number; week: number; lifetime: number } | null
>(null);
const petHoverTimerRef = useRef<number | null>(null);

const handlePetAmbientEnter = useCallback(() => {
  if (petHoverTimerRef.current !== null) return; // 计时器已 armed
  if (ambientStats !== null) return; // 已显，不重启
  petHoverTimerRef.current = window.setTimeout(async () => {
    petHoverTimerRef.current = null;
    try {
      const [today, week, lifetime] = await Promise.all([
        invoke<number>("get_today_speech_count"),
        invoke<number>("get_week_speech_count"),
        invoke<number>("get_lifetime_speech_count"),
      ]);
      setAmbientStats({ today, week, lifetime });
    } catch (e) {
      console.error("ambient stats fetch failed:", e);
    }
  }, 3000);
}, [ambientStats]);

const handlePetAmbientLeave = useCallback(() => {
  if (petHoverTimerRef.current !== null) {
    window.clearTimeout(petHoverTimerRef.current);
    petHoverTimerRef.current = null;
  }
  setAmbientStats(null);
}, []);

useEffect(
  () => () => {
    if (petHoverTimerRef.current !== null) {
      window.clearTimeout(petHoverTimerRef.current);
    }
  },
  [],
);
```

#### 挂到 Live2D wrapper

在 `<div style={{ position: "relative", flexShrink: 0, height: "220px" }}>` 上加 `onMouseEnter` / `onMouseLeave`：

```tsx
<div
  style={{ position: "relative", flexShrink: 0, height: "220px" }}
  onDoubleClick={handlePetDoubleClick}
  onMouseEnter={handlePetAmbientEnter}
  onMouseLeave={handlePetAmbientLeave}
  onContextMenu={...}
>
```

#### 卡片渲染（Live2D wrapper 内，close 按钮前）

```tsx
{ambientStats !== null && (
  <div style={{
    position: "absolute", bottom: 10, left: "50%",
    transform: "translateX(-50%)",
    padding: "4px 10px", borderRadius: 10,
    background: "var(--pet-color-card)",
    border: "1px solid var(--pet-color-border)",
    fontSize: 11, lineHeight: 1.3,
    zIndex: 55, opacity: 0.92,
    pointerEvents: "none", userSelect: "none", whiteSpace: "nowrap",
    display: "flex", gap: 8, alignItems: "center",
  }}>
    <span><muted>今日</muted> 🐾 {today}</span> ·
    <span><muted>本周</muted> {week}</span> ·
    <span><muted>累计</muted> {fmtCount(lifetime)}</span>
    {companionshipDays >= 0 && <>· <span>✦ {companionshipDays} 天</span></>}
  </div>
)}
```

lifetime ≥10000 显 `1.2k` 这种简化避免占位过宽。

## 关键设计

- **3s 计时器，路过不触发**：cursor 横扫 Live2D 区不会立刻触发；只有"真停在 pet 上端详"才弹卡片 —— 区分"路过"和"想看"。 3s 是 owner 自然停留下限（如肉眼看心情 / 等下一句对话），不会觉得"等太久"，又不会"碰一下就闪"。
- **lazy fetch on tick**：3 个 IPC 命令仅在 3s 计时器到点才并行发；owner 永不 hover 时 0 IPC 噪声。首次卡片可能闪 ~50-200ms（等 IPC 返回）也接受 —— ambient 而非命令响应。
- **mouseleave 立刻清 + 卡片消失**：下次 hover 重新启 3s + 重新 fetch = 数据始终新鲜。不缓存 ambientStats —— owner 重新 hover 通常隔了至少几十秒，新鲜度优先。
- **pointerEvents none 让卡片不接 hover**：mouseLeave 由 wrapper div 上判断 relatedTarget 是否还在 wrapper 内。卡片 pointer-events: none → 卡片元素对 cursor 不可见 → cursor 实际仍在 wrapper 矩形内 → 不会误触发 mouseLeave 关掉自己。
- **底部居中位置**：避开顶部已挂 4 个 chip / pill（任务 pill 左、🐾 + ✦ 右、▶| 右）+ 右下心情 widget。底部中央是 pet 视觉留白区，卡片自然居中悬浮。
- **4 段而非 3 段**：spec 写"今日 / 本周 / 累计"三段，但既有 chip 数据是 🐾 (today) + ✦ (companionship)；为了"聚合既有 chip 数据让 owner 一瞥全景"，把 ✦ 陪伴 N 天也加进去 —— 今日 / 本周 / 累计 (开口) + ✦ N 天 (陪伴)。companionshipDays === -1 (未抓到) 时少一段。
- **不挂 onClick**：ambient 是被动 hint，不是 deeplink trigger。owner 想看详情仍走 ✦ chip click → Panel 人格 tab 既有路径。卡片即看即走。
- **opacity 0.92 不到 1**：留一丝"浮起"感与 pet 主体区分，但不到半透；fontSize 11 + 4×10 padding 让卡片小巧不喧宾夺主。

## 不做

- **不加 fade-in 动画**：3s 等待本身就是一种"渐入"节奏；DOM mount 即显，节省 keyframe 注入。
- **不显多于 4 段**：再加 30 天 / 心情 dominant 等会让卡片太宽 + 太杂。要更深 stats 走 Panel。
- **不缓存 ambientStats 跨 hover**：mouseleave 即清。新鲜数据 > 闪烁缓存。
- **不让卡片可点击跳 Panel**：见"不挂 onClick"理由。
- **不挂全屏 click-outside dismiss**：mouseleave 已足够；用户拖窗 / 焦点离开都走 leave 路径。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.14s
- 改动 ~80 行（state + 三 handler 40 + 卡片 JSX 50）；既有 sparkle / mood widget / 任务 pill / 🐾 / ✦ chip / 收起按钮 / 右键菜单 / 双击 happy motion 全部不动。
- 4 段卡片在 220×宽 Live2D 区底部居中预期可放下；whiteSpace nowrap 防换行；过宽时自然超出（owner 改写 max-width 后续 iter 可调）。

## TODO 状态

剩 1 条留池：
- PanelMemory 类目内"📈 7 天 churn" mini sparkline

## 后续

- 卡片底部加一行 "上次 N 分钟前 来找你 · 心情 X" 让 ambient 不只是数字 + 而是"宠物状态摘要"。
- 3s 阈值 + opacity 做成 settings 可调（有人想更快显 / 更慢显 / 更隐）。
- ChatMini hover 同样配 ambient？（场景：owner 已开始对话，hover 历史区时浮一条"今天聊了多少句"统计）—— 但 ChatMini 已挂 hover history，要小心不冲突。
