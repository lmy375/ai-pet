# PanelMemory item "📋 复制 detail.md 全文" 按钮

## 需求

iter #172/#180 hover preview tooltip 显前 600 字 detail.md（够扫读）。
iter #194 list 层显字数指示器（够判断长度）。🚀 按钮在系统编辑器打
开 detail.md。但纯"我想把整篇 detail 复制到外部 markdown 笔记 / chat /
issue"路径没入口 —— 用户得用 🚀 打开 → 全选 → 复制 → 切回，三步。
补一键复制按钮。

## 实现

### 后端

`src-tauri/src/commands/memory.rs`：

- 新 tauri command `memory_read_detail_full(detail_path)`：
  - 与 `memory_read_detail` 同 path traversal 防御（`..` / 绝对路径 reject /
    canonicalize / starts_with）
  - 但不截断 —— 直接返完整内容
  - 失败容忍：读不到返空串（前端按"无内容"处理 + toast 提示）

`src-tauri/src/lib.rs`：注册 `memory_read_detail_full`。

### 前端

`src/components/panel/PanelMemory.tsx` item action 行加新按钮：

- 仅在 `detailSizes[item.detail_path] > 0` 时显（与 iter #194 字数指示
  共用 detailSizes 缓存；0 字 / 不存在的 detail 不浮按钮）
- onClick：
  - invoke memory_read_detail_full
  - 命中非空 → `navigator.clipboard.writeText(content)` + toast
    `已复制 detail.md 全文（X 字）`
  - 空 → toast `detail.md 内容为空 / 读不到`
  - 抛错 → toast `复制失败：${e}`
- 视觉：📋 emoji，与既有 🚀 / 🔗 / 编辑 / 删除 按钮平级
- 全 catKey 都可用（不仅 butler_tasks）—— 任何 memory item 都有 detail.md

## 验证

- `cargo check`：clean
- `npx tsc --noEmit`：clean
- 行为：
  - detail.md 已写有内容 → 行末显 📋 按钮
  - detail.md 不存在 / 空 → 不显（detailSizes 命中 0）
  - 点击 → 剪贴板装完整 detail.md 文本 + toast 显字数
  - 粘到外部编辑器 / chat → 完整 markdown 形态
  - 不影响 hover preview（仍 600 字截断；两条路径独立）

## 不在本轮范围

- 没做"复制 detail.md + 元数据 frontmatter"：detail.md 当前是纯 markdown
  无 frontmatter；future 若加 yaml head 再考虑包不包
- 没做"截断到 N 字版"（如复制 1000 字精简版）：用户要短的看 hover preview；
  完整路径就给完整
- 没把 button 改成 dropdown 选"前 600 字 / 全文"两 mode：单按钮单语义
  更直接，dropdown 选择是冗余
- 没在 PanelTasks 同款补按钮：PanelTasks 已有"复制完整描述"+"复制 detail.md"
  在展开态卡片里 (line ~4185/4255 区域)；list 层 PanelTasks 操作菜单较少
  改动，scope 不扩

## TODO 池剩余

- PanelSettings "📋 导出全部 settings 为 markdown" 按钮
- PanelPersona "重置 SOUL.md 为内置默认" 按钮
- PanelTasks 任务卡 hover preview tooltip 显 "⚡ NOW" 标记状态
- PanelChat session 切换草稿提示 toast
