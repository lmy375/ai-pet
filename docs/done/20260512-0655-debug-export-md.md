# PanelDebug stats 一键导出 markdown

## 需求

PanelDebug 各种 stats chip / pending review / tone snapshot / 工具风险表
散在各处，用户排查 / 贴 issue 时要手动复制零碎数据。加一个"📋 导出快照
MD"按钮，把当前调试视图整体拼成 markdown 写剪贴板。

## 实现

`src/components/panel/PanelDebug.tsx`：

- 新 `buildDebugMarkdownSnapshot()` useCallback：从已有 state 派生 markdown
  字符串。结构：
  - 顶部时间戳 + 陪伴天数 + 主动开口计数（today / week / lifetime）
  - 工具缓存（turns / hits / calls）
  - 心情 motion 命中（with_tag / without_tag / no_mood）
  - proactive 出口分布（spoke / silent / error）
  - env 工具引用（active_window / weather / upcoming_events / memory_search）
  - prompt tilt 分布（restraint / engagement / balanced / neutral）
  - tone snapshot（JSON code fence）
  - 待审核工具调用 list（如有）
  - 待提醒 list（前 10 + 余条提示）
  - 工具风险偏好覆盖（仅 mode !== "auto" 的行）
  - 宠物最近 5 句 speech
- 新 state `debugExportMsg: string` 显 3.5s 反馈
- toolbar 加 "📋 导出快照 MD" 按钮 + 旁边短反馈文本

## 设计选择

- 只导出 PanelDebug 当前可见 state（不再额外 fetch）—— 用户看到什么导什
  么，避免"导出值与画面不一致"
- 工具风险表只列 override（mode !== auto），让 issue 阅读者关注用户改动
  过的工具，不被 BUILTIN_TOOL_NAMES 的默认表淹没
- speech 取尾部 5 条而非全部：speech_history 体积大，前面没 context 时
  贴 issue 反而冗长
- tone 直接 JSON code fence：内部结构丰富（band / 强度 / dimensions），
  人读 + machine parse 都方便

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - PanelDebug 顶部 toolbar 出现 "📋 导出快照 MD" 按钮
  - 点击 → 剪贴板得 markdown；旁边显 "已复制调试快照 markdown 到剪贴板"
    绿色 3.5s
  - 粘到 GitHub issue / Notion → 渲染整齐
  - tone 为 null 时（数据未到）该段省略
  - 全 auto 的工具风险表 → "工具风险偏好覆盖" 段省略
  - 复制失败（私密浏览权限拒）→ 红字 "复制失败：..."

## 不在本轮范围

- 没把 LLM 日志详情塞进 markdown：日志体量大，每条单独有 "📋 复制 cURL"
  路径已够；快照里塞太多会让贴 issue 时被截断
- 没做导出 JSON / YAML 多格式选项：markdown 是 issue 友好默认；用户需要
  机器格式可看现有 stats / structured API
- 没做"导出 + 截图"：跨 platform 截图 API 在 Tauri 还在 plugin 阶段，
  本轮先解决纯文本

## TODO 池剩余

- PanelTasks ⌘N 全屏 quick-add 模态
