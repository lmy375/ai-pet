# PanelMemory 显存储占用

## 需求

memory 写多了就该 consolidate（merge / 删过期）。但用户无视觉信号知道"多
了"。在 PanelMemory 头部加一行 `📚 N 条记忆 · 💾 X.X MB (M 个文件)` 让感
知具体化。

## 实现

### 后端

`src-tauri/src/commands/memory.rs`：

- 新 struct `MemoryDiskUsage { total_bytes, file_count }`
- `memory_disk_usage()` 命令递归扫 `~/.config/pet/memories`，显式 stack 模拟
  递归（防深层 detail.md 嵌套打爆真递归栈），加总 file.len() + count
- 错误（dir 不存在 / 权限）→ Err 透传；实操 memories_dir() 在上面 ensure 过

`src-tauri/src/lib.rs` 注册命令。

### 前端

`src/components/panel/PanelMemory.tsx`：

- 新 state `diskUsage: { total_bytes, file_count } | null`，挂载时 invoke 拉一次
- 头部 message 行下方加 stats 块（仅 diskUsage 非 null 时渲染防 layout 抖动）：
  ```
  📚 N 条记忆    💾 X.X MB (M 个文件)
  ```
- tooltip 显原始字节数 toLocaleString 让用户能精确看到

新 `formatBytes(n)` helper：1024 基数（与 Finder / du -h 习惯一致），按
B/KB/MB/GB 选最大单位 + 1 位小数精度。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - PanelMemory 顶部加载完 → 显"📚 N 条记忆 💾 1.3 MB (42 个文件)"
  - hover 显原始字节数 toLocaleString
  - 用户 add/edit 后不强刷（不是高频精确数据）—— 重新打开 panel 时刷
  - memories dir 为空 → "0 B (0 个文件)"

## 不在本轮范围

- 没做 auto-refresh after edit：memory_edit 频率不高，下次开 panel 自然刷
- 没显 categories 分项占用：用户决策点只是"总体大小"，分项详情没必要
- 没在阈值（如 > 50 MB）变红 / 弹"建议 consolidate"提示：避免给用户增焦虑，
  数字+趋势他们自己能判断

## TODO 池剩余

- ChatMini 桌面气泡可拖动
- /image 在 ChatPanel 桌面也生效
- 设置页 motion_mapping group datalist
