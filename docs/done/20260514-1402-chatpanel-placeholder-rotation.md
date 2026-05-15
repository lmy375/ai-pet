# 桌面 ChatPanel 输入框 placeholder 轮播

## 背景

TODO（auto-proposed 之前）：

> 桌面气泡 placeholder 轮播：每 30s 切换提示文案让输入框少点"待机寡淡感"。

桌面宠物窗口的输入框在空闲时显一句固定 placeholder「说点什么…（可粘贴 / 拖入图片）」—— 功能信息一次就够，看久了视觉很闷。让 placeholder 在几句"陪伴感"文案间轮换，与 ChatMini idle fade、Live2D 双击 happy motion、任务完成 sparkle 一起构成"桌面有活气"的微交互层。

## 改动（frontend only）

### `src/components/ChatPanel.tsx`

**1. 模块顶常量**

```ts
const CHAT_INPUT_PLACEHOLDERS: string[] = [
  "说点什么…（可粘贴 / 拖入图片）",
  "今天感觉怎么样？",
  "想聊点啥？",
  "需要帮忙做什么？",
  "随便聊聊，我陪着 🐾",
];
const CHAT_INPUT_PLACEHOLDER_ROTATE_MS = 30_000;
```

**第一句保留功能性提示**（粘贴 / 拖入图片）让新用户初见时能学到能力；后续 4 句 conversational 风。30s 一换是经验值：长到不打扰阅读 / 思考、短到放置一会儿就能看到下一句。

**2. 组件内 state + effect**

```ts
const [placeholderIdx, setPlaceholderIdx] = useState(0);
const inputEmpty = input.length === 0;
useEffect(() => {
  if (!inputEmpty || isLoading) return;
  const id = window.setInterval(
    () => setPlaceholderIdx((i) => (i + 1) % CHAT_INPUT_PLACEHOLDERS.length),
    CHAT_INPUT_PLACEHOLDER_ROTATE_MS,
  );
  return () => window.clearInterval(id);
}, [inputEmpty, isLoading]);
```

`!inputEmpty || isLoading` 时不挂 interval —— 用户在打字 / 流式中 placeholder 看不到，省 re-render。从空 → 非空时 cleanup 把上一个 interval 销掉，下次回空 effect 重挂 → idx 从当前值继续（不 reset 到 0，让"我又回到空状态了"自然衔接到刚才轮到哪句）。

**3. textarea placeholder 替换**

```tsx
placeholder={
  isLoading
    ? "宠物正在回复中..."
    : CHAT_INPUT_PLACEHOLDERS[placeholderIdx]
}
```

loading 文案不变 —— 流式回复中是"宠物在干活"的明确状态，不该掺感性文案。

## 不做

- **不让 placeholder 跟随心情 / 时间段**（如早上 "早安 🌞"、晚上 "夜里聊吗"）。需要接 mood / clock 状态，且边界多（晨昏过渡 / 心情切换瞬间 placeholder 抖动）。当下 5 句固定中性，先看用户反馈。
- **不重设 idx 到 0 当 input 重新清空**。让"我又空了→ placeholder 从刚才那句继续"是更自然的连续性，不是从开头开始一遍。
- **不让用户配置文案**。5 句容量已包含"功能 + 友好"两端，自定义是 over-engineering 的入口；如未来 demand 起来再加 settings 配置。
- **不动 PanelChat 输入框**。Panel 是阅读 / 编辑长 prompt 的场景，固定功能 placeholder（含 `/` `@` ⌘K ⌘B 等快捷键速查）信息密度高，rotation 会让用户学不到那些键位。
- **不写测试**。前端无 vitest；逻辑是单 useEffect + setInterval，行为清晰。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~30 行（常量 18 + state + effect 12 + placeholder 替换 3）；既有 historyCursor / paste / drop / image-thumbs 等行为全部不动。

## 后续

- placeholder 跟随宠物 mood 切换语气（happy 时 "想分享什么？"、tired 时 "想吐槽就吐"）。
- 节日 / 时段定制句子（春节 / 早晚 / 周末），与 morning_briefing 同信号源。
- PanelChat 输入框对偶轮播？慎重 —— Panel 是 power-user 长 prompt 场景，placeholder 在功能性上承担"快捷键提示"职责。
