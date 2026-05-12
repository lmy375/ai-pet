# 设置页"重启 pet 窗口"按钮

## 需求

某些字段需要重启窗口才能生效：`live_2d_model_path` 切换不同 Live2D 模型、
`motion_mapping` 需要重新挂 listener、`minWidth/minHeight` 等 native window
属性。当前用户得手动 quit 整个 app 再重开。提供一键重启 pet 窗口的按钮。

## 实现

### 后端

`src-tauri/src/commands/window.rs` 新加 `restart_pet_window(app: AppHandle)`：

- get_webview_window("main") → 若在，`win.close()`（忽略错）
- `WebviewWindowBuilder::new(app, "main", url)` 用与 tauri.conf.json main 块同
  样的尺寸 / decorations / transparent / alwaysOnTop / resizable / skip_taskbar / shadow
  配置重建。min_inner_size 220x350 与 conf 同步。
- macOSPrivateApi 已经在 conf 全局开了，builder 这里无需重复
- 不触碰 panel / debug —— 它们自己的 webview window，让用户 panel 状态保留

`src-tauri/src/lib.rs` 注册 `commands::window::restart_pet_window`。

### 前端

`src/components/panel/PanelSettings.tsx`：

- 新 state `restartArmed: boolean` + timer ref
- Live2D 模型 section 在路径 input 下方加 🔄 重启 pet 窗口按钮
- armed 二次确认：first click → 红填充 + setMessage "再点一次确认重启 pet
  窗口（5 秒内）。先点最下方『保存』让配置落盘。"；5s 内再点 → invoke +
  setMessage "已重启 pet 窗口"
- tooltip 强调"先保存才能让新值生效"+"panel / debug 窗口不动"

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 改完 live_2d_model_path → 保存 → 点 🔄 重启 pet 窗口 → 红 armed → 再点 →
    桌面 pet 窗关 + 立即重开
  - 新窗口用新 settings.live_2d_model_path 加载新 Live2D 模型
  - panel / debug 窗口不动
  - 5s 不点 → armed 自动撤回
  - main 窗已被用户关掉时仍能点重启（会 spawn 新）

## 不在本轮范围

- 没在重启前自动 save settings：让用户主动保存符合"先确认改动 → 再生效"流
  程；tooltip 已提醒
- 没做"软重启"（仅 reload webview 不重建 window）：TODO 池里已加 follow-up
  为 panel 自身 reload；本轮先 ship hard restart 路径

## TODO 池清空 → 自主提案

按规则 #1 提 5 条新需求（已写入 TODO.md）：

1. 重启按钮加 reload 当前窗口语义（panel 自身 reload）
2. PanelTasks 任务行右键菜单
3. PanelChat 跨会话搜索 hit 高亮
4. image_model 测试按钮加 lightbox 大图
5. ChatMini 流式中显当前 tool 名
