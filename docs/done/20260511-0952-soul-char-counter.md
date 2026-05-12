# SOUL.md 编辑器加字数 counter

## 需求

SOUL.md 是 system prompt，每次 chat 都注入到第一条 message。长 prompt × 每轮
× 多个 session → token 成本累积明显，但当前编辑器没字数反馈，用户不知道自己
写的有多长。补一个 counter，与 PanelTasks detail / PanelMemory title 同款，
让用户掌握 prompt 长度。

## 实现

`src/components/panel/PanelSettings.tsx`：textarea 下方加 `<div>{len} 字</div>`，
三档颜色 + tooltip 解释。

```ts
const SOFT = 500;
const HARD = 1000;
const color =
  len >= HARD ? "var(--pet-tint-red-fg)"
  : len >= SOFT ? "var(--pet-tint-yellow-fg)"
  : "var(--pet-color-muted)";
```

阈值参考：
- 500 字 ≈ ~750 token（中英混排），多 session × 多轮还可接受
- 1000 字 ≈ ~1500 token，开始挤其它 system layer（mood / persona / 历史）的预算

tooltip 文案根据档位给具体引导："建议精炼" / "可以继续但留意" / 仅显技术说明。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 输 100 字 → muted "100 字"
  - 输 600 字 → 黄色 "600 字" + tooltip"偏长"
  - 输 1200 字 → 红色 "1200 字" + tooltip"建议精炼"
  - 字数右对齐 + 等宽字体（与其它 counter 一致）

## 不在本轮范围

- 没把 counter 改成 token 估算（粗略 / 精确分词器都要引入计算库），先用字数
  做近似指标
- 没在保存按钮做"长度阈值警告" —— counter 颜色已经足够提醒；硬性 block 反而
  限制用户高级写法（详尽 SOUL 用 1k+ 字也合理）
