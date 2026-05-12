# Motion mapping 全部演示按钮

## 需求

PanelSettings motion mapping 4 行已有逐项 "▶ 试一下" 按钮，但用户改完映射
想一眼看完整套效果得手动点 4 下。加一个"▶ 全部演示一遍" 按钮按顺序自动
触发 Tap → Flick → Flick3 → Idle。

## 实现

`src/components/panel/PanelSettings.tsx`：

- 新 state `demoingMotions: boolean`
- Motion 映射段头从单 label 改成 flex 行：左侧 label，右侧"▶ 全部演示一遍"
  按钮（accent 配色，与"▶ 试一下"区分）
- onClick handler：
  - setDemoingMotions(true)
  - for-loop 顺序 invoke `trigger_motion` 每个 semantic + await 1.6s 间隔
  - try/catch 吞掉单步错误（一个 motion 触发失败不应中断整个 demo）
  - 完成后 setDemoingMotions(false)
- 1.6s 间隔取多数 motion 单次 < 1.5s 的经验值 —— 等当前 motion 自然衰减后
  再切下一个，避免叠播让用户看不清

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点 "▶ 全部演示一遍" → 桌面 Live2D 顺序播 4 个 motion，每个间隔 1.6s
  - 演示中按钮 disable + 灰底 + 文案换 "演示中…"，再点无效
  - 全跑完 ~6.4s 后按钮回到可点态
  - 某个 motion 没映射 / 触发失败 → console.error 但 demo 继续往下走
  - 与现有逐项 "▶ 试一下" 按钮共存，互不冲突

## 不在本轮范围

- 没改 trigger_motion 后端：现有命令已足够，前端 sequencing 即可
- 没加"自定义播放间隔"：1.6s 是经验值，加 UI 配置过度设计
- 没让 motion 映射变化时自动 demo：保存才生效，自动 demo 会让"调参中"
  各种试错刷屏

## TODO 池剩余

- PanelChat 顶部 session 横排 tab-like 标签栏
- PanelDebug LLM 日志多 chip 过滤
- PanelTasks origin 过滤 chip
