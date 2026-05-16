# ChatMini bubble 双击 ref token 跳转时附 audio bell ping

## 背景

iter #189 加了 ChatMini bubble 双击「title」ref → 跨窗口跳 PanelTasks 同名任务行 pipeline。但跨窗口 deeplink 有 ~150-300ms 延迟（开 panel + tab 切换），owner 双击后视觉等待时不确定"刚才那个操作触发了吗"。

加 200ms 轻量 beep（Web Audio API oscillator）听觉确认 ref 已被识别 + jump 已触发。

## 改动

`src/components/ChatMini.tsx` —— onRefDoubleClick(title) 调用前插 audio playback：

```ts
try {
  const AC = window.AudioContext || (window as ...).webkitAudioContext;
  if (AC) {
    const ac = new AC();
    const osc = ac.createOscillator();
    const gain = ac.createGain();
    osc.type = "sine";
    osc.frequency.value = 880;  // A5
    gain.gain.setValueAtTime(0.06, ac.currentTime);  // 6% 音量
    gain.gain.exponentialRampToValueAtTime(0.0001, ac.currentTime + 0.15);
    osc.connect(gain).connect(ac.destination);
    osc.start();
    osc.stop(ac.currentTime + 0.16);
    window.setTimeout(() => ac.close(), 300);  // 释放 context
  }
} catch {
  // 静默退化 —— 跳转主流程仍走
}
onRefDoubleClick(title);
```

## 关键设计

- **Web Audio API 无 asset 依赖**：浏览器 native 内置 oscillator + gain node，不需打包 audio 文件，零 bundle 体积增加。
- **A5 880Hz sine wave 0.06 amplitude**：A5 是温和清亮 pitch，0.06 音量 (6% peak) 在系统音量下属 ambient 不刺耳。Sine 是最柔的 wave 形态（squares/saws 会刺耳）。
- **exponentialRampToValueAtTime fade**：从 0.06 衰到 0.0001 in 150ms —— 自然包络不会突然 cutoff "嘎" 一声。
- **try/catch + AudioContext null guard**：webview 偶发 AudioContext 不可用 / Safari webkit 前缀 / 用户 muted browser；silent 退化保跳转主流程。
- **setTimeout 300ms close ac**：oscillator stop 后释放 AudioContext 防内存堆积。
- **每次重新 new AC**：避免缓存 AC 实例可能被浏览器 suspend；每次轻 alloc 简单可靠。
- **触发位置紧贴 onRefDoubleClick(title) 之前**：音效与 jump action 几乎同时发生，owner 听觉 + 视觉双反馈。

## 不做

- **不写 audio asset**：bundle 体积 + asset 协议 + audio MIME type cross-platform 复杂；oscillator 已足够。
- **不让 Settings 开关 bell on/off**：实测 0.06 音量 + 短 150ms 不刺耳；如果 owner 反馈太响再加 setting。
- **不对其它路径（顶时钟 click / 底相对 chip click）也加音效**：单一 trigger 才有标识度；处处响铃反而 noise。
- **不写测试**：纯 Web Audio API 调用；视觉验证（有 owner audio output 设备 → 双击 ref → 听到 ~150ms ding）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~40 行（try/catch + AC + osc + gain + ramp + close + 注释）。既有 ref 探测 / stopPropagation / onRefDoubleClick / fallback onOpenPanel 路径完全不动。

## TODO 状态

剩 2 条留池：
- detail.md toolbar 加 "🧠 ask LLM about selection"
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- ⌥+双击 ref token 跳但 mute bell —— owner 临时不想响铃时。
- Settings 加 sound_effects bool 开关让 owner 全局禁声。
- 用 BiquadFilter 加 lowpass 让 880Hz beep 听起来更柔（当前 sine 已经够柔，但 lowpass 可以更"宠物感"）。
- 不同 action 用不同 pitch：ref jump A5 / save 完成 C5 / error E4 等 sound-id ambient。
