# 054 · pet figure idle 微动画

模型 `miku.model3.json` 自带 `Idle` motion + `EyeBlink` parameter group，但
启动后 figure 静止。pixi-live2d-display 自动巡回 Idle 未生效。

- ✅ part1：`Live2DCharacter.tsx` 在 `onModelReady` 后显式
  `model.motion("Idle", undefined, 1)` kick-start（priority 1=IDLE 让
  Tap/Flick priority 2 能打断）。tsc + vite build clean。
- ⏳ part2：dev session 视觉验证 — 不够则加 setInterval 兜底 / 调研 lib。
- ⏳ part3：mood→motion 已在 useMoodAnimation.ts；扩 keyword + CPU 监测。
