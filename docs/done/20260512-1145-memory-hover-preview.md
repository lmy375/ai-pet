# PanelMemory hover detail.md preview

## 需求

每条 memory item 只显 title + 简短 description；detail.md 内容要点编辑或
🚀 外部打开才能看。用户想快速浏览"这条 memory 实际写了什么"得多走几步。
加 hover 500ms 后浮 tooltip 显 detail.md 前 600 字符 preview。

## 实现

### 后端 `src-tauri/src/commands/memory.rs`

- 新 `memory_read_detail(detail_path: String) -> Result<String, String>`：
  - 路径守门：拒 `..` 段 + 拒绝绝对路径前缀 `/`
  - canonicalize 双方后再 `starts_with(mem_dir)` 兜底防 symlink 逃逸
  - 文件不存在 / 读失败 → 返空字符串（"无预览可显"非错误）
  - 按 char（非 byte）截断到 600，避免切到中文 / emoji 中间
- 注册到 `lib.rs`

### 前端 `src/components/panel/PanelMemory.tsx`

- 新 state：
  - `previewHoverKey: string | null` —— 当前 hover 触发的 detail_path
  - `previewCache: Record<string, string>` —— path → 内容缓存，避免重复 IPC
  - `previewHoverTimerRef` —— 500ms debounce timer
- `startPreviewHover(detailPath)`：清旧 timer + 设新 500ms timer →
  setPreviewHoverKey + 缓存未命中时 invoke `memory_read_detail`
- `endPreviewHover()`：清 timer + 清 hover key
- unmount cleanup 清 timer
- 每条 item 容器加 `onMouseEnter / onMouseLeave`，并在 active+content 非
  空时浮渲染 tooltip：
  - position absolute, top:100%, left/right:0, marginTop:4
  - 头部 muted 字显文件路径，下面 pre-wrap monospace 内容
  - maxHeight 220 + overflowY auto 防长 detail 撑死
  - pointerEvents none —— 鼠标不被 tooltip 截走（用户 hover bubble 中
    间不会"飘起"丢 hover）

## 验证

- `cargo check` clean
- `npx tsc --noEmit` clean
- 行为：
  - hover 一条 memory 0.5s 内 → tooltip 浮出，显 detail.md 前 600 字符
    （超长加 "…"）
  - 鼠标离开 → tooltip 立即消失，timer 清
  - 同 memory 再次 hover → 缓存命中，瞬间显（无延迟）
  - detail.md 文件为空 / 不存在 → 无 tooltip（无信号 = 无打扰）
  - 试图传 `../../../etc/passwd` → 后端 Err "invalid detail_path"
  - 关 panel 重开 → 缓存清；首 hover 再发 IPC

## 不在本轮范围

- 没让 tooltip 支持 markdown 渲染：纯 plain text + monospace 已能满足
  "扫一眼内容"诉求；markdown 渲染要 parseMarkdown，与 inline 解析有冲突，
  单独需求
- 没做 hover preview 在 search 结果列表：那是 flatten 视图，行高不一样；
  本轮只在 category section item 上加；统一可作后续
- 没存 preview 到 sessionStorage 持久化：缓存 session 内有效已够；持久化
  会让 stale detail 缠绕

## TODO 池剩余

- ChatMini 桌面气泡 markdown 块级语法
- PanelTasks 归档独立 tab
