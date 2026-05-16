# 桌面 pet 主区"🐾 今天主动 N 次" ambient chip

## 背景

TODO 上 auto-proposed 一条："桌面 pet 主区顶部浮『今天 N 条 · 主动 M 次』ambient 小字：让 owner 一眼看到今日陪伴密度（与 ✦ 陪伴天数 chip 对偶位）。"

桌面 pet 窗口右上已有 ✦ 陪伴天数 chip 显累计天数。但每天宠物主动找了多少次 owner 看不到 —— 只能切到 Panel「人格」tab 看 speech stats 详情。

实施时把"今天 N 条 · 主动 M 次"双计数简化为单 chip "🐾 今天主动 N 次"。理由：
- `get_today_speech_count` Tauri 命令已现成（返 proactive 主动开口次数）。
- "今天 N 条聊天" 需要扫 today's sessions items 数，无单 IPC 路径，要新加后端。
- 单 chip 已是质的飞跃 —— v1 不需要双数。

## 改动

### `src/App.tsx`

#### usePollingState 拉今日 proactive 次数

紧贴 `companionshipDays` poll 之后：

```ts
const { data: todaySpeechCount } = usePollingState(
  () => invoke<number>("get_today_speech_count"),
  600_000,  // 10min 同 companionshipDays 节奏；proactive 一天 ~10 次，60s 太频
  -1,
);
```

#### 🐾 chip JSX

紧贴 ✦ companionship chip 之前（视觉上落在 ✦ 左侧 `right: 76px`，给 ✦ 的 36px + ~40px chip 宽留出空间）：

```tsx
{todaySpeechCount > 0 && (
  <div
    onClick={() => {
      try {
        window.localStorage.setItem(
          "pet-panel-deeplink",
          JSON.stringify({ tab: "人格", ts: Date.now() }),
        );
      } catch {}
      openPanel();
    }}
    title={`今天宠物主动来找你 ${todaySpeechCount} 次（不含你主动开口）。点开「人格」tab 看完整 speech 统计。`}
    style={{
      position: "absolute",
      top: "8px",
      right: "76px",
      padding: "3px 9px",
      borderRadius: "12px",
      background: "var(--pet-color-card)",
      border: "1px solid var(--pet-color-border)",
      color: "var(--pet-color-muted)",
      fontSize: "11px",
      fontWeight: 600,
      opacity: 0.6,
      // ...
    }}
    onMouseOver / onMouseOut={...opacity 0.6 ↔ 1...}
  >
    🐾 {todaySpeechCount}
  </div>
)}
```

## 关键设计

- **chip > 0 才显**：proactive 早上还没开口前 count = 0 → 显 "🐾 0" 是噪音。`-1` fallback（usePollingState 未抓到时的兜底值）也不显（与 ✦ chip 同模式：`companionshipDays >= 0`）。
- **10min 轮询节奏**：与 ✦ companionship 同。proactive 一天 typical ~10 次，每 10min 检查变化够新；60s 太频繁浪费 IPC + Tauri JS↔Rust 切换。
- **right: 76px 位置**：左于 ✦ chip (right: 36px) + 给 ✦ chip 实际宽度 ~30-40px 留出 8-10px gap。视觉上"🐾 N · ✦ M"成左右排，与 owner 阅读"今天活跃 · 累计陪伴"语义同序。
- **click 跳「人格」tab**：Persona tab 已有完整 speech stats（今日 / 本周 / 累计 + 小时分布）。chip 是 ambient 入口，详细数据走 deeplink 跳过去看。
- **opacity 0.6 → 1 hover**：与既有 ✦ chip 同视觉风格，让 ambient 元素半透不抢主视觉 (Live2D 模型 + ChatMini 是焦点)。
- **不写 "今天 N 条聊天" 双数**：see 背景段。`get_today_session_message_count` 后端命令不存在；要加 backend → 扫 today's sessions items.user/assistant len → 累加，是独立 iter 工作量。v1 单 chip 已交付 80% 价值。

## 不做

- **不接 backend 命令 "今天聊天条数"**：scope 控制。如果用户反馈"想看今天聊了多少"再加。
- **不写测试**：纯 polling + 条件渲染；既有 ✦ chip 同模式视觉验证。手动测：让宠物主动开口几次 → 10min 内 chip 显数 → click 跳 Persona tab → 确认数字对得上。
- **不持久化用户偏好"显/不显"**：chip 极简（11px font + opacity 0.6）+ 仅 > 0 才显，不构成显著视觉打扰。
- **不在 mini chat 内 show**：mini chat 已显聊天 bubbles 本身；统计性 chip 应在 ambient 区域。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~70 行（usePollingState 10 + chip JSX 50 + 注释 + 位置常量）；既有 ✦ companionship chip / 任务 pill / MoodWidget / 收起按钮等其它 chip 位置不变。

## TODO 状态

6 条 auto-proposed 已完成 3 条（含 stale 移除 1 条：PanelSettings 🔌 测试 LLM 连通性按钮 — line 1431 早已存在 `🧪 测试 chat` 按钮带 reply preview / elapsed ms / error display），余 3 条留池：
- proactive prompt 加最近 24h 完成任务
- PanelChat sort chip
- detail.md textarea ⌘D 复制当前行（前轮已完成实际是 5 → 3 条）

实际池：
- proactive prompt 加最近 24h 完成任务
- PanelChat sort chip

## 后续

- 加 "今天聊天 N 条" 配合显示双数 —— 需新加 backend `get_today_chat_message_count`（扫 today's sessions items.role in [user, assistant] 累加）。
- chip 切到 ✦ 右侧（视觉上"今日 → 累计"时间维度递增更自然） + 收起按钮移到再右一格。需 right 坐标重排。
- chip 点击改为 hover 浮气泡显小图："📈 今日 7:00 / 9:00 / 14:00 主动" timeline；与 Persona tab speech_hourly 同源。
