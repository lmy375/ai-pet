# README 补回最近用户可见亮点

## 背景

TODO.md 指引："如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。"

过去多轮 cron loop 里 ship 了不少用户可见特性，README 没同步：
- @ task ref picker（聊天输入框 @ 触发）
- 复制消息 ⇧+点击带 session title + timestamp 元数据
- task detail.md markdown 工具栏（B / • / 🔗）
- task_archive ↩ 恢复到队列
- 归档搜索框
- 今日会话 chip
- SQLite 持久化分层（11 轮迭代成果）+ 专用 LLM 工具

## 改动

`README.md`：

### 「1. 被动聊天」section 加 3 条
- @ 提及任务 ref
- 复制消息带元数据（⇧+点击）
- 今日会话 chip

### 「4. 宠物管家」 task_archive 行扩展
- 归档区顶部新增搜索框（按 title/description 过滤）
- 单条「↩ 恢复」按钮
- 新增「任务详情 markdown 工具栏」行

### 新增「8. 持久化分层」 section
分三段说明：
- memory 职责（user_profile / ai_insights / general）
- SQLite 表结构（butler_tasks / todo / task_archive 独立表，mood / persona_summary / daily_plan / daily_review_<date> 走 kv_state）
- 专用 LLM 工具（butler_task_edit / todo_edit + PanelDebug 占比 chip）

## 不做

- 不动既有 1-7 section 结构
- 不新增技术栈条目（rusqlite 算实现细节，README 技术栈表保留 high-level）

## 验收

- README 渲染正常（无 markdown 语法错）
- 新内容与已 ship 功能一一对应

## 完成

- [x] 6 处 README 增量
- [x] 移到 docs/done/
