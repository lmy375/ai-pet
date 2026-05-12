# PanelTasks priority input ▲▼ 微调按钮

## 需求

inline / quickAdd 创建表单 priority 字段是 `type="number"`，原生 spinner
在 WKWebView 上偏小+视觉淡。鼠标用户调 1 阶不便。补显式 ▲▼ 按钮。

## 实现

`src/components/panel/PanelTasks.tsx` 两处（inline create form + quickAdd
modal）都把 input 包进 flex 容器，右侧加 ▲▼ 按钮：

- ▲ → `setPriority(p => Math.min(PRIORITY_MAX, p + 1))`
  - disabled 当 `priority >= PRIORITY_MAX`
- ▼ → `setPriority(p => Math.max(0, p - 1))`
  - disabled 当 `priority <= 0`
- title tooltip 含"数字大 = 不紧急" / "数字小 = 紧急"提示，提醒方向
- 视觉小巧（padding 0 8px / fontSize 10），与 input 等高

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - inline 表单展开 → priority 字段旁 ▲▼ 显
  - quickAdd modal 打开 → 同款 ▲▼
  - 当前 P3 点 ▲ → 变 P4，再 ▲ → P5...
  - P9 点 ▲ → disabled
  - P0 点 ▼ → disabled
  - 仍可手敲数字 + Enter 提交（不破坏 keyboard 路径）

## 不在本轮范围

- 没改 task 卡 priority badge 上的 picker（那是 P0-9 完整选择器）
- 没让 ▲▼ 长按重复（mousedown-hold）：< 10 阶范围，逐点击足够
- 没集成色阶视觉（按钮自身染色按当前 priority）：保持 muted 色让操
  作灯不抢 badge 焦点
- 没改 bulk action 那个 priority input（bulk 已有"全部 +1 / -1"按钮，
  不必再加 micro spinner）

## TODO 池剩余

- PanelChat 自定义模板 "🛠 管理" modal
- PanelChat marks modal entry "🗑" 移除标记
- PanelMemory butler_tasks "📋 复制完整 prefix + topic"
