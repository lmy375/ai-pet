# PanelTasks 任务行 origin chip（📨 TG）

## 背景

任务面板顶部有 origin 过滤 chip（📨 TG vs 💻 面板，二元集合 OR 语义），让用户筛选"只看 TG 派的"或"只看面板创的"。但**每条任务行内**没有 origin 标识：扫一行只能看到 title / status / due / 创建时间。想知道这条是不是 TG 派的，要么先用顶部 filter 切，要么展开看 raw_description 找 `[origin:tg:...]` marker。

## 改动

`src/components/panel/PanelTasks.tsx`：任务卡 meta 行 "创建于 ..." 之后，加 conditional origin chip：

```tsx
{t.raw_description.includes("[origin:tg:") && (
  <span onClick={...activate origin filter tg}
        title="本任务从 Telegram 派出..."
        style={pill style with tint-blue bg/fg}>
    📨 TG
  </span>
)}
```

- 仅 TG marker 存在时渲染（面板创建是默认值，不显避免噪音）
- 点击：把 tg 加进 originFilter set（如已在 set 则 noop，不 toggle 掉）—— "我看到这条是 TG → 顺手集中看所有 TG 任务"
- 不传播 click 到父行（stopPropagation）—— 父行 click 是展开详情，origin chip 应独立交互
- pill 风格用 tint-blue，与顶部 origin chip 同色域

## 不做

- 不加 💻 面板 chip：缺省没 origin marker 即面板创建，加上反而每行都显"💻 面板"信息噪音
- 不动 raw_description 解析（继续用 includes 简单 substring 匹配；与既有顶部 filter 同算法）
- 不持久化 / 不写测试（纯 UI 显隐 + 单次 setState）

## 验收

- `npx tsc --noEmit` ✅
- 切「任务」tab，TG 派出的任务行有 📨 TG chip；面板创建的不显
- 点 chip → 顶部 origin filter 加 tg，列表自动 filter

## 完成

- [x] conditional chip render
- [x] click → set originFilter
- [x] 移到 docs/done/
