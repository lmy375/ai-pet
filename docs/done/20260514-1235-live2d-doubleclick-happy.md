# Live2D 双击触发 happy motion

## 背景

TODO（auto-proposed 之前）：

> Live2D 双击 happy motion：用户双击宠物触发 happy / surprised 一次动作，增强"互动反馈"。

GOAL.md 强调"实时陪伴型 AI 桌面宠物"+"UI 美观可爱"。被动陪伴已经做了很多（任务 sparkle / idle fade / mood widget），但"用户主动触发宠物"还缺一个肌肉记忆 IM 操作 —— 双击。其他桌面宠物（Tamagotchi / 小米米兔 / Live2D Cubism 默认范例）都把"双击 / 戳"作为最基础的互动手势。本次补这一刀。

## 改动

### `src/hooks/useMoodAnimation.ts`

**导出 `MotionGroup` 类型** —— 让 App.tsx 等调用方有类型保证不传错语义键。

**新增公开函数 `playPetMotion(model, semantic, mapping)`**：

```ts
export function playPetMotion(
  model: any,
  semantic: MotionGroup,
  mapping: Record<string, string> | undefined,
) {
  if (!model) return;
  const group = resolveGroupName(semantic, mapping);
  try {
    model.motion(group, undefined, 2);
  } catch (e) {
    console.debug("motion trigger failed:", e);
  }
}
```

复用既有 `resolveGroupName` 让用户的 `settings.motion_mapping` 自定义模型映射也生效。priority 2 = NORMAL（与既有 mood 触发同优先级，不抢高级动作）。

私有 `triggerMotion` 是按 mood + motion 派生 group 的复合路径；`playPetMotion` 是「调用方直接给语义键」的简化路径。两条路径并存，各自语义清楚。

### `src/App.tsx`

1. **新增 `lastTapAtRef` cooldown**：600ms 内连点忽略，防用户疯狂双击刷动画。短于自然 mood 节奏的最低周期；长到一个动作能播完。

2. **新增 `handlePetDoubleClick` 回调**：

```ts
const handlePetDoubleClick = useCallback((e) => {
  const target = e.target as HTMLElement;
  if (target?.closest?.("[data-no-pet-dblclick]")) return; // 守门 hook
  const now = Date.now();
  if (now - lastTapAtRef.current < 600) return;
  lastTapAtRef.current = now;
  playPetMotion(modelRef.current, "Tap", settings.motion_mapping);
}, [settings.motion_mapping]);
```

`data-no-pet-dblclick` 是预留 hook 让未来子元素显式拒绝事件冒泡（当前所有浮标已自有 stopPropagation，无需挂此 attribute）。

3. **Live2D wrapper div** 加 `onDoubleClick={handlePetDoubleClick}`：

```tsx
<div style={{ position: "relative", flexShrink: 0, height: "220px" }}
     onDoubleClick={handlePetDoubleClick}>
  <Live2DCharacter ... />
  ...
</div>
```

子区域浮标（任务 pill / MoodWidget / 收起按钮 / sparkle overlay）的点击事件已 stopPropagation，不会被识别为 Live2D 双击。

## 不做

- **不变化 motion**（每次都 Tap）。"双击 = 高兴" 是直觉化的最低契约；随机 motion 会让用户搞不清"双击到底会怎样"。如果未来要彩蛋（连点 3 次进 Flick3 焦虑、震一下进 Tap 等），可叠加但不该作为 default。
- **不写连点跳到 Flick3 / Idle 的"宠物烦了"反应**。当前节奏先稳定"happy" 这个核心信号；用户反馈再考虑。
- **不阻塞当前 motion**。priority 2 = NORMAL；正在播 motion 时双击不打断（用户双击多按几次会自然连贯）。
- **不在 panel 加双击**。Panel 里没有 Live2D 渲染区；交互在桌面 pet 窗才有意义。
- **不挂 ChatMini bubble 的双击**。那里已是"打开 Panel"的双击语义；不冲突。
- **不动 useMoodAnimation 公开 hook 行为**。新 `playPetMotion` 是独立 export，没改 hook 内部 listen / mood-derive 路径。
- **不写测试**。前端无 vitest；handlePetDoubleClick 是 5 行交互逻辑，cooldown + early-return 易于 verify。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s
- 改动 ~30 行（hook export 14 + App handler 15）；既有 useMoodAnimation lifecycle 不动。

## 后续

- "💗 互动好感度" 计数：双击触发的事件可入 record_bubble_liked 同模式的统计，让 PanelPersona 多一项"今日被点过 N 次"。
- 长按 / 三连击触发特殊 motion（Flick / Flick3）—— 但只在用户表达需求后再加，避免动作组哗众取宠。
- 双击触发音效（轻量 chime）—— Live2D 已有 lipsync 音轨能力，复用同 audio context 不会增加 deps；但需要新音频资产，本次不做。
