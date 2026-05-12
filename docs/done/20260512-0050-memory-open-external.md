# PanelMemory 单条 detail.md 外部打开

## 需求

每条记忆有 `detail_path`（指向 `~/.config/pet/memories/<cat>/<title>.md`），
当前要编辑只能走"点编辑 → modal textarea → 保存"。想在 VSCode / Typora /
iA Writer 等专业编辑器里大段写时没有快捷入口。加 🚀 按钮调系统默认 .md
关联打开。

## 实现

### `src-tauri/capabilities/default.json`

- 既有 `opener:default` 仅给 `openUrl`；为 `openPath` 加显式权限：
  - `opener:allow-open-path` —— 让 frontend `openPath()` 通过
  - `opener:allow-reveal-item-in-dir` —— 留给后续"在 Finder 里显示"按钮
    （本轮未用但顺手加）

### `src/components/panel/PanelMemory.tsx`

- import `openPath` from `@tauri-apps/plugin-opener`
- 每条记忆 action 行（pin / 编辑 / 删除）之间插入 🚀 按钮：
  - onClick 调 `openPath(item.detail_path)`
  - try/catch 包，成功 setMessage 短反馈 "已请求系统打开 <filename>"（2.5s）
  - 失败 setMessage 显错误（4s）—— 极旧 memory 可能 detail.md 还没生成；
    或系统未关联 .md handler

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 点 🚀 → 系统默认 markdown editor 跳起并打开该 detail.md
  - 关闭后再点 → 仍正常（OS 复用同一窗口或新开看用户偏好）
  - 老 memory 没 detail.md → "打开失败：..." 红色 message
  - 主面板 modal 编辑路径仍可用 → 两种编辑方式互不干扰
  - tooltip 显完整 detail_path 让用户能确认要打开的文件

## 不在本轮范围

- 没做"打开后 reload 当前 memory" —— 用户在外部 editor 写完保存，panel
  下次切换 / refresh 自动 list_memories 重新读，不必专门 watch fs
- 没做"自定义 editor 命令"（如固定用 VSCode）：`openPath` 走 OS 关联，
  跟随用户已有偏好就够；强制选某 editor 反而限定
- 没在桌面 mini / 任务 detail 同步加：本轮专注 memory；后续 task detail 编
  辑器也可加同款按钮（detail_path 同模式）

## TODO 池剩余

- PanelChat session tab 栏右键菜单
- PanelTasks detail.md markdown 预览
- PanelDebug 工具风险 inline 调整
