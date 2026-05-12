# 桌面窗口可缩放（部分实现 ChatMini 独立摆位）

## 需求

TODO 原话："桌面气泡硬贴在 Live2D 下方。allow 用户独立移动气泡到屏幕右下
角等位。"

## 范围权衡

完整实现需要：
1. 新建 Tauri webview 窗口（transparent + always-on-top）放 ChatMini
2. 跨窗 emit/listen 同步 useChat 的 messages / streaming / cancel state
3. 两窗口可见性 / 位置持久化
4. UI 入口（设置 / 桌面按钮）控制 attach / detach

这是一项跨多轮的架构性工作 —— 移到 GOAL.md 待确认。

本轮 ship 一个小赢面：pet 窗口改成 `resizable: true`，加上 `minWidth: 220 / minHeight: 350`，让用户至少能拖角拉大窗口给 ChatMini 留更多显示空间。

## 实现

`src-tauri/tauri.conf.json` main window 配置：

```diff
+ "minWidth": 220,
+ "minHeight": 350,
- "resizable": false,
+ "resizable": true,
```

minWidth / minHeight 守住"Live2D 220px + ChatMini ≥ 1 行 + ChatPanel input"三段
的最小可用面积，防止用户把窗口拖到不可读尺寸。

`docs/GOAL.md` "待确认"段加 "ChatMini 独立窗口" 整体描述，记录"完整方案需
要哪些 piece + 已 ship 的小赢面"以方便 owner 决定何时启动。

## 验证

- `cargo check` clean
- 行为：
  - 启动桌面 pet → 窗口边角可拖拉大 / 小
  - 拖小到 < 220x350 → 系统级 enforce 不让继续缩
  - 拉大 → Live2D 区固定 220px，ChatMini flex:1 自动占满剩余空间 → 历史可
    见行数变多

## 不在本轮范围

- 真正的"独立窗口拖到屏幕任意位"：等 owner 确认 GOAL 后再启动多窗口架构
- 没改桌面 drag handle 区域：当前 Live2D 区可触发 window dragging，仍是
  整窗一起移动；分离 drag handle 等多窗实现到位再处理

## TODO 池清空 → 自主提案

按规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. 设置页"重启 pet 窗口"按钮
2. ChatMini 图片"另存为"
3. PanelChat 全局清屏（清所有 session）
4. 任务详情阅读态字数 counter
5. ChatMini hover 气泡显时间戳
