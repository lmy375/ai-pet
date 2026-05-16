# 桌面 pet hover 3s ambient 卡片末加 🌐 时区 chip

## 背景

桌面 pet 主区 hover 3s 浮的 ambient 卡片已有 4 段（今日 / 本周 / 累计 主动开口数 + ✦ 陪伴天数）。owner 远程办公 / 频繁出差时常想"我现在本机几点 + 时区是啥" —— pet 卡片是常驻 ambient 显示位置，自然把时区/本机时间加进去。

## 改动

### `src/App.tsx`

ambientStats 卡片末尾追加 5th 段：

```tsx
<span style={{ color: "var(--pet-color-border)" }}>·</span>
<span style={{ color: "var(--pet-color-muted)" }} title="...">
  🌐{" "}
  {(() => {
    const now = new Date();
    const hh = String(now.getHours()).padStart(2, "0");
    const mm = String(now.getMinutes()).padStart(2, "0");
    const tz = (() => {
      try {
        return Intl.DateTimeFormat().resolvedOptions().timeZone;  // "Asia/Shanghai"
      } catch {
        const off = -now.getTimezoneOffset();
        const sign = off >= 0 ? "+" : "-";
        const oh = String(Math.floor(Math.abs(off) / 60)).padStart(2, "0");
        const om = String(Math.abs(off) % 60).padStart(2, "0");
        return `UTC${sign}${oh}:${om}`;  // fallback "UTC+08:00"
      }
    })();
    const tzShort = tz.split("/").pop() ?? tz;  // "Shanghai"
    return `${hh}:${mm} ${tzShort}`;
  })()}
</span>
```

显示例：`🌐 14:32 Shanghai` / fallback `🌐 14:32 UTC+08:00`。

## 关键设计

- **Intl.DateTimeFormat IANA tz**：直接拿 "Asia/Shanghai" 等 city 缩写 —— owner 阅读直觉好于 "+08:00" offset 串。
- **failsafe offset 串**：Intl 不可用（极少数老 webview）时 fallback `UTC+HH:MM`。两种格式都能让 owner 知道时区。
- **last segment "/Shanghai"**：长 IANA 串切成短 city —— "Asia/Shanghai" → "Shanghai"，"America/Los_Angeles" → "Los_Angeles"。城市名是 readable identifier。
- **muted color**：与 4 段已有 stats 同 muted；卡片整体 ambient 不抢戏。
- **inline 第 5 段**：不开新行，让卡片仍是单行紧凑（占宽 ~120-150px 内）。
- **重算 on render**：ambient 卡片每次 hover 重新渲（state 由 hover 触发），HH:MM 即时反映当前时间。无 setInterval 浪费 cycle。

## 不做

- **不显秒**：HH:MM 已够 ambient 用；秒级粒度噪音 + 需要 setInterval 每秒重渲。
- **不显完整 IANA**：长 "Asia/Shanghai" 撑爆卡片宽。short city 已够辨识。
- **不绑 click → 弹时区切换 modal**：app 时区跟系统时区一致；不该应用层覆盖。
- **不写测试**：纯 inline 字符串 + Intl API；视觉验证（hover pet 3s → ambient 卡片末应见 🌐 chip）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.23s
- 改动 ~30 行（IIFE timezone 计算 + chip render + 注释）。既有 4 段 ambient stats / hover 3s 触发 / 卡片样式完全不动。

## TODO 状态

剩 1 条留池：
- butler_task 行 [reminderMin: N] chip click 弹快速编辑

## 后续

- 加 UTC offset 显在 IANA city 旁（"14:32 Shanghai +08"）让跨 tz coordinate 更易心算时差。
- 双时区显（owner 在 SF 但同事在 Shanghai）—— settings 加 "secondary timezone" 字段。
- 时区切换检测（owner 改系统时区）后弹一条 banner "你刚切到 Pacific —— 任务 deadline 也跟着换了哦"。
- ChatMini 顶部静态 chip 也加 🌐，让收起态 pet 也能直接看（hover 3s 才显的本 chip 收起时不可见）。
