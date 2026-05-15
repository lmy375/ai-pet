# 桌面 pet 顶栏陪伴天数 ✦ chip

## 背景

TODO（上一轮 auto-proposed）：

> 桌面 pet 顶栏 hover 显"✦ N 天" 陪伴 chip：当前需开 PanelPersona 才看到陪伴天数，桌面 hover 浮一下是常用 ergo。

GOAL.md 第一条「打造一个实时陪伴型 AI 桌面宠物，为用户提供情绪价值」。陪伴天数是宠物 vs 用户关系的核心 metric —— 但当下需要"打开 Panel → 切到「人格」tab" 两步操作才能看到。在桌面 pet 顶栏直接显个小 chip，让用户随时看到"今天是和宠物的第 N 天"，是情绪连接的微小但有意义的强化。

## 改动（frontend only）

### `src/App.tsx`

**1. 轮询拉数**

```ts
const { data: companionshipDays } = usePollingState(
  () => invoke<number>("get_companionship_days"),
  600_000, // 10 min: day-granular，跨日 ≤ 10min 更新到位
  -1,      // 初始 -1 = 未 fetch → chip 不渲染（避免空 chip 占位）
);
```

10 分钟轮询：陪伴天数本质上是日级别的数值（一天才变一次）；10min 在 midnight 跨日时能 ≤ 10min 内更新到 N+1。0.5 / 1h 也可，10min 是稳健默认。

**2. chip 渲染（紧贴收起按钮左侧）**

```tsx
{companionshipDays >= 0 && (
  <div
    onClick={() => {
      try {
        localStorage.setItem("pet-panel-deeplink",
          JSON.stringify({ tab: "人格", ts: Date.now() }));
      } catch { /* fallback */ }
      openPanel();
    }}
    title={companionshipDays === 0 ? "今天与你初识 🐾..." : `已陪伴 ${companionshipDays} 天 🐾...`}
    style={{
      position: "absolute",
      top: "8px", right: "36px",  // 收起按钮 22px + 6px 间隔
      padding: "3px 9px", borderRadius: 12,
      background: "var(--pet-color-card)",
      border: "1px solid var(--pet-color-border)",
      color: "var(--pet-color-muted)",
      fontSize: 11, fontWeight: 600,
      opacity: 0.6, transition: "opacity 120ms ease-out",
      cursor: "pointer", boxShadow: "var(--pet-shadow-sm)",
      zIndex: 60,
    }}
    onMouseOver={(e) => (e.currentTarget as HTMLDivElement).style.opacity = "1"}
    onMouseOut={(e) => (e.currentTarget as HTMLDivElement).style.opacity = "0.6"}
  >
    ✦ {companionshipDays}
  </div>
)}
```

**关键设计**：

- **opacity 0.6 ↔ 1 hover 切换**：与既有收起按钮 ▶| 同节奏（视觉风格一致）；默认半透明不抢戏，hover 抬亮让"我能点"明显。
- **不显「天」字**：tiny chip 优先 compact，11px 字号 + "✦ N" 两 token 已传达；天数语义靠 ✦ 符号（陪伴星）+ tooltip 全文（"已陪伴 N 天 🐾"）。
- **天数 = 0 文案**：tooltip "今天与你初识 🐾" —— 与 PanelPersona 的 0 天文案一致，让 onboarding 第一天的用户体验温暖。
- **点击 deeplink 跳人格 tab**：用户看陪伴天数最自然的延伸是"点开看更多关于这只宠物 / 我的关系" —— 「人格」tab 含 persona summary / mood sparkline / 最近常用工具，完整呈现。复用既有 `pet-panel-deeplink` 协议（"人格" 已是 valid TABS 之一）。
- **`>= 0` 渲染门**：-1 兜底"未 fetch / IPC 失败"，渲染时整个 chip 不存在（避免短暂"✦ -1" 闪烁）。

## 不做

- **不动 MoodWidget**：它的双击展开 sparkline 是另一条独立的"看心情趋势"路径；这俩 chip 一上一下，语义并行不重复。
- **不暴露 settings 开关**：opacity 0.6 默认已克制；用户嫌烦可未来加 localStorage `pet-companion-chip = "off"`，本次不做。
- **不写测试**：前端无 vitest；逻辑是 IPC + render，无新算法。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~70 行（usePollingState fetch + chip render block）；既有 task pill / MoodWidget / sparkle / 收起按钮位置不变（chip 在 task pill 左上 / MoodWidget 左下 / 收起按钮 ▶| 右上的"夹空间"里）。

## TODO 状态

- 本轮实现 1 条。
- TODO 剩 2 条：会话标题 LLM 自动重写按钮 / PanelMemory item description 双击 inline 编辑。

## 后续

- 跨日 (midnight) 立即刷新而非等下一轮 poll：listen "day-changed" 事件并 trigger refresh。
- chip 旁加 install_date 完整日期（YYYY-MM-DD）作 hover 附加信息。
- 周年纪念日（30 / 100 / 365 天）chip 加 sparkle 边框装饰。
