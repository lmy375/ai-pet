# PanelTasks 📋 audit chip per-signal filter pills（iter #600）

## Background

iter #596 加 PanelTasks 📋 audit chip：hover tooltip 含 5 signals。
但 tooltip 文本不可点 — owner 看到信号无法直接触发对应 filter。
本 iter 升级 chip 为可展开 cluster — click 展开后每 signal 一个
mini-pill：📌/💤 click 触发 pinnedFilter / idleFilter；🚀/🏷/✅
informational。

让 audit chip 从「数字看板」变「navigation entry」。

## Change

`PanelTasks.tsx`：

1. 新 session 态 `auditExpanded: boolean`（紧贴 completedListExpanded）
2. 重写 audit chip：单 chip → inline 可展开 cluster
3. 主 chip click → toggle 展开
4. 展开后 5 个 signalPill：
   - `📌 N`: click → toggle pinnedFilter（仅 count>0 可点）
   - `💤 N`: click → toggle idleFilter（仅 count>0 可点）
   - `🚀 N` / `🏷 N` / `✅ N`: disabled (informational)
5. 「📋 copy」按钮在展开尾，click 复制 md summary（原 click 行为
   从 main chip 移到这）

## Visual

收起态：
```
📋 audit · 24
```

展开态：
```
📋 audit · 24 | 📌 12 | 💤 5 | 🚀 3 | 🏷 8 | ✅ 4 | 📋 copy
```

active filter pill（如 pinnedFilter ON）走 amber 色填底；inactive
muted card 底；disabled informational pill 同 inactive 样但 cursor
default。

## Key design decisions

- **session 态 not persisted**：每次 panel mount 默认收起 — chip-bar
  紧凑优先；owner 想看 signals 主动展开
- **filter pill disabled 当 count=0**：count=0 时 pinnedFilter toggle
  也无意义 → disable + cursor default
- **copy 按钮独立 chip**：原 click 行为（复制 md）从 main chip 转
  移；保留入口但 click main chip 现为展开切换 — owner 想复制需展开
  + 点 「📋 copy」（2 步）。tradeoff：discoverability 略降但行为
  consistent（main chip click = expand）
- **`pinnedFilter` / `idleFilter` toggle 复用既有 setter**：与 chip-bar
  其它单独 📌 / 💤 chip 同后端 — 一致行为
- **`onClick={(e) => e.stopPropagation()}` per pill**：防 click pill
  时也触发 main chip 的 toggle expand

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 改造仅 PanelTasks，复用既有 setter，无
  cross-component race

## Future iters (out of scope)

- **🚀 signal click 切到「今日 + P7+」filter**：当前 informational；
  若以后加 today + priority 联合 filter 入口可挂上
- **🏷 signal click 弹 modal 显近 N 条 rename**：与 TG /recent_renames
  桌面对偶。需 modal 框架
- **chip 展开态持久化**：localStorage 记忆 owner 偏好。但 chip-bar
  紧凑优先 — 默认收起合理
- **keyboard shortcut**：⌘⇧A 切换 audit expansion — 但 panel 已有
  多 shortcut，按需
