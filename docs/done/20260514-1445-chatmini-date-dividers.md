# ChatMini 跨日消息分隔条

## 背景

TODO（本轮 auto-proposed）：

> ChatMini 跨日分隔条：消息时间戳跨日时插入"今天 / 昨天 / MM-DD" 居中横条，让用户回滚长历史时一眼分辨"哪条是今天 / 昨天"。

ChatMini 已经有 `[HH:MM]` 小角标 + 自适应折叠（< 60s 同 role 隐中间），但跨日的边界靠时间戳数字记心。20+ 条消息且分布在 3 天里，用户回滚找"昨天宠物提了个建议"时只能逐个看 ts 的日期推算。加居中"今天 / 昨天 / MM-DD" 分隔条比 ts 数字更直观，是 IM 系（iMessage / Telegram / Slack）的通用 affordance。

## 改动（frontend only）

### `src/components/ChatMini.tsx`

**1. 两个 pure helper**

```ts
function dateKeyFromTs(ts: string | undefined): string | null {
  if (!ts) return null;
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return null;
  // YYYY-MM-DD
  return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;
}

function formatDateDividerLabel(dateKey: string, now: Date = new Date()): string {
  // 今天 / 昨天 / 同年 MM-DD / 跨年 YYYY-MM-DD
}
```

`dateKeyFromTs` 走本地时间组件（getFullYear / getMonth / getDate）—— 与既有 `formatBubbleTimestamp` 同时区基线，避免 UTC 偏移误判跨日。`formatDateDividerLabel` 取 `now` 注入便于将来 vitest 落地直接 unit test 边界（昨天 / 今年 / 跨年）。

**2. 跨日检测 + Fragment 包裹**

在 `visibleItems.map` 头部：

```ts
const curDateKey = dateKeyFromTs(m.ts);
const prevDateKey = idx > 0 ? dateKeyFromTs(visibleItems[idx - 1].ts) : null;
const showDateDivider = curDateKey !== null && curDateKey !== prevDateKey;
```

第一条（`idx === 0`）若有有效 ts → 显（让对话起点也有日期锚）。ts 无效（curDateKey null）静默跳 —— 与既有 ts 标签"无效 → 不显" 同语义边界。

**3. 分隔条 render**

行外用 `<Fragment>` 包裹（key 从 div 提到 Fragment），让 React list 仍稳定唯一：

```tsx
<Fragment key={`${m.role}-${idx}-${text.length}-${imgs.length}`}>
  {showDateDivider && (
    <div aria-hidden style={{ display:"flex", alignItems:"center", gap:8, fontSize:9,
        color:"var(--pet-color-muted)", letterSpacing:0.5,
        margin:"8px 4px 4px", userSelect:"none" }}
         title={`本组消息从 ${curDateKey} 开始`}>
      <span style={{ flex:1, height:1, background:"color-mix(in srgb, var(--pet-color-border) 70%, transparent)" }} />
      <span style={{ flexShrink:0 }}>{dateLabel}</span>
      <span style={{ flex:1, height:1, background:"color-mix(in srgb, var(--pet-color-border) 70%, transparent)" }} />
    </div>
  )}
  <div className="pet-mini-row" ... >
    ...
  </div>
</Fragment>
```

视觉：两段 1px hairline（70% border alpha 比纯 border 更克制不喧宾）+ 中间日期文字（9px、muted、letter-spacing 0.5）。margin top 8 / right-left 4 / bottom 4 让分隔条在上一组结束后留一拍呼吸感，但不远到像独立 section。

`aria-hidden` + `title` attr：屏幕阅读器不读分隔（避免重复 announce），鼠标 hover 能拿到机器可读的 YYYY-MM-DD（用户偶尔需要精确日期不用心算）。

## 不做

- **不动 PanelChat 大聊天**。PanelChat 的 items 数组没有 ts 字段（独立 schema），加 date divider 要先做"按 idx 对齐到 messagesRef 的 ts"映射或给 items 加 ts —— 独立 scope。
- **不接 i18n**。"今天 / 昨天" 是中文写死；与 README / 其它中文 UI 文案保持一致。English 版属于全应用 i18n 任务，单独做。
- **不显时间区域 (HH:MM-HH:MM 这段日内多少时间)**。本特性的本意是日界面，时间维度由既有 `[HH:MM]` 角标 + 自适应折叠负责。
- **不让 divider 可点击 / 互动**。`aria-hidden` 让屏幕阅读器跳过；鼠标 hover 仍可见 tooltip。点击进交互态会引入"是否折叠该日"等复杂度，非本特性范围。
- **不写测试**。前端无 vitest；两个 helper 是 12-15 行的纯函数，逻辑明显；将来一接入 vitest 就能直接 pin（已 export friendly）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~70 行（helper 35 + 检测 4 + divider JSX 30 + Fragment 包裹）；既有 ts 标签 / 自适应折叠 / 右键菜单 / 双击 ref / search hit 高亮全部不动。

## 后续

- PanelChat 大聊天也加 date divider（需要先打通 items↔messagesRef ts 映射 或扩 items schema）。
- vitest 落地后给 `formatDateDividerLabel` / `dateKeyFromTs` 加边界测试（昨天 / 跨年 / 闰年 2-29 等）。
- 长 history 跨年时 click divider 跳到对应日期的第一条（与 PanelChat 跨会话搜索同模式）。
