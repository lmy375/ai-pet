# 桌面 pet 双击 happy motion 后偶尔随机播一句鼓励 line

## 背景

桌面 pet 双击触发 happy "Tap" motion 已存在，但只播动作不带回应。owner 双击 pet 时心理预期 "宠物会有点反应"，单纯动作有点冷。

加 ~30% 概率 push 一句鼓励 line 到 ChatMini（appendAssistant 软消息），让双击有 mini reaction —— 不每次都触发免噪音轰炸。

## 改动

### `src/App.tsx`

#### 1. 文学短句库

```ts
const happyLinesRef = useRef([
  "🐾 嘿，你看到我啦 ✨",
  "✨ 摸摸我会让我心情更好的～",
  "🐾 双击我也是一种问候呢",
  "💫 看你心情不错的样子",
  "🌸 想我啦？我也想你",
  "🐾 谢谢你来打个招呼",
  "✨ 难得你这么主动找我玩",
]);
```

7 条 2-3 字 emoji 起头 + 7-12 字主体的轻松 line。ref 保持 stable references（不每次 render 重建数组）。

#### 2. handlePetDoubleClick 加 30% 概率分支

```ts
if (Math.random() < 0.3) {
  const lines = happyLinesRef.current;
  const line = lines[Math.floor(Math.random() * lines.length)];
  appendAssistant(line);
}
```

只在 motion 播完后 30% 触发；每次随机选 line —— 重复 10 次平均得 3 条不同 line。

## 关键设计

- **30% 概率**：每次双击都 push 太烦；< 50% 是"偶尔"，> 20% 是"常常"。30% 让 owner 双击 ~3 次平均听一句，节奏合适。
- **7 条 line ample variety**：随机选每条 ~14% 几率，重复短期内不易感知；可后续扩到 15-20 条。
- **emoji + 短句 + 平实语气**：与既有 systemNote 软消息风格一致 —— "🎨 图片生成失败：..." / "⚡ 标记..." 等 emoji 起头短句。
- **存 ref 而非 const 数组**：let runtime 在 component lifetime 内固定 array 引用；将来如果想动态加 line（owner 自定义鼓励词），同位置 mutate ref.current 即可。
- **appendAssistant 软消息渠道**：与 reminderMin / 倒计时 / NOW marker 提醒等 ChatMini 软推路径一致 —— owner 视觉熟悉。
- **不打开 Live2D proactive 模式**：仅 ChatMini 推一条，不抢 LLM 焦点。

## 不做

- **不让 LLM 实时生成 line**：随机 line 库零延迟 + 零 token；走 LLM 反而慢 / 贵 / 偶尔说错。
- **不调整概率随 mood**：scope creep；本 iter 固定 30%。
- **不写测试**：纯 Math.random + array index；非 deterministic 不好 unit test。手动验证（双击 10 次看 ~ 3 次有 line）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~25 行（happyLinesRef 8 + 30% 分支 + appendAssistant 调用 + dep array 加 appendAssistant + 注释）。既有 lastTapAtRef / playPetMotion / motion_mapping 路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelMemory item description 行级 hover preview 含完整内容
- ChatPanel 输入框历史栈 hover 显 idx / total
- detail.md 编辑器 ⌘K 唤起 task quick-find palette
- PanelTasks 列表行 hover idx / total 角标

## 后续

- 双击 line 与宠物 mood 联动："悲伤"心情态下不推或推安慰 line；"高兴"态用 happyLines；"无聊"态用 "动一动我吧"...
- 双击 line 库 owner 可在 settings 自定义（YAML / textarea）—— 让宠物说 owner 喜欢的话风。
- 双击 line 累计触发次数显在 PanelDebug stats（与 todaySpeechCount 同源），让 owner 看自己"调戏"频率。
