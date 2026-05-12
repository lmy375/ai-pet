# PanelMemory item 行 detail.md 字数指示

## 需求

iter #175 给 PanelTasks detail.md 编辑面板加 > 5000 字 banner，引
导用户精简。但用户得先展开任务 + 进入编辑态才看到 banner。在
PanelMemory item 列表层直接显字数（"📄 N 字"小灰字），让用户扫一
眼就识别哪条 detail.md 偏长 + 跳进去精简，与编辑态 banner 互补。

## 实现

### 后端

`src-tauri/src/commands/memory.rs`：

- 新 tauri command `memory_detail_sizes() -> HashMap<String, usize>`
  - 一次性扫所有 category 的所有 item 的 detail.md，算 `content.chars().count()`
    （与编辑态 counter 同方法，对中文 / emoji 正确）
  - path traversal 防御（`..` / 绝对路径直接 skip + canonicalize +
    starts_with 检查），与 memory_read_detail 同源
  - 失败容忍：单文件读不到 → 该 path 不进 map（前端按"无字数信号"处理）
  - 返回 `Record<detail_path, char_count>`

`src-tauri/src/lib.rs`：注册 `commands::memory::memory_detail_sizes`。

### 前端

`src/components/panel/PanelMemory.tsx`：

- `useCallback` 加入 react import
- 新 state `detailSizes: Record<string, number>` + `refreshDetailSizes()`
  helper
- useEffect 依赖 `[refreshDetailSizes, index]` —— 挂载即拉，index 变化
  （edit / consolidate / fire 后 loadIndex 推进）也自动重刷字数
- 失败 silently 忽略 → 保留旧 map，不让短暂 IO 抖动退化全部 indicator
- item meta 行（`detail_path | 更新于 ...` 行）末尾加条件渲 indicator：
  - 仅 size > 0 显（缺失 / 0 字不渲染，避免 "0 字" 占视觉位）
  - 三档配色与编辑态 counter 同语义：< 2000 muted / 2000-5000 amber /
    > 5000 red 加粗
  - 文案 `· 📄 N 字`；title tooltip 显完整 N + 等级解释 + 引导（"> 5000
    字建议精简，编辑面板会浮 banner"）

## 验证

- `cargo check`：clean
- `npx tsc --noEmit`：clean
- 行为：
  - 全新 panel 挂载 → 立即拉 detail_sizes（< 100 文件 / ms 级 IO）
  - 短 detail（< 2000 字）→ 显 `· 📄 500 字` muted 灰
  - 偏长 detail（2000-5000 字）→ amber 黄
  - 超长 detail（> 5000 字）→ red 红加粗，提醒精简
  - detail 不存在 → 不渲染（meta 行只有 path + 更新时间）
  - 在 PanelTasks 编辑 detail.md 保存 → PanelMemory 切回时（index reload）
    数字同步更新
  - 用户 ⌥ 关机 / 私密模式 / 路径解析失败 → 该条 item 不显字数，不闪 error

## 不在本轮范围

- 没把字数信号集成进 hover preview tooltip 头部（已显 `📄 detail_path`）：
  hover preview 本来就显前 600 字，已经隐含长度感；list 字数指示是
  "无 hover 也能看"
- 没做"全部 detail 总字数"统计（panel header 显汇总）：磁盘占用 disk
  usage 已经显字节数；总字符数与磁盘占用强相关，重复信号
- 没做点击字数 → 跳到 task detail 编辑态：item 行已有"编辑"按钮直
  达，跳转 alias 边际
- 没做 byte size 显示：UTF-8 中文 3B / 字符，byte 与字符数差 3x，
  与编辑态 "X 字"一致比 byte 更直观

## TODO 池剩余

- PanelTasks header 加 "P0 一键过滤" 单独 chip
- PanelTasks 手动标 done 时弹 "可选 result 摘要" 输入对话框
- PanelChat 长 session 翻历史浮 "↓ 跳到最新" 浮动按钮
- PanelMemory butler_tasks item 描述里的「task title」ref token 也渲 hover preview / 双击导航
