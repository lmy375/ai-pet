# 014 · PanelReports — 管家工作回查面板

011 scheduled_report / 012 deferred_task / 013 briefing 都已把"宠物为用户跑活儿"的能力打通，但输出全部停留在一次性 utterance — 用户隔几天想回看「上周宠物给我跑了哪些活儿、结论是什么」没有任何入口。管家做完就忘，信任无从积累。

需求：
- 新 panel `PanelReports` 与 PanelMemory / PanelTasks 并列。
- 写入源：011 scheduled_report fire / 012 deferred_task fire / b6a193e butler_task 归档；每条 entry 含 ts、source-type、spec text、output text、success_or_fail。
- 列表默认时间倒序；顶栏 chip filter 按 source-type 切（scheduled / deferred / butler_archive / all）。
- 单条点击展开完整 output；含工具调用链时折叠显示「调用了 N 个工具」chip，可二次展开。
- 30d 滚动归档（与 stale_butler_archive_days 同一窗口节奏），归档项不在默认列表显示但 chip 可切「含归档」。
- TG 对偶：`/reports_list [N]` 列最近 N 条；`/report <id>` 展开单条详情。
- 不引入新数据 store — 直接读 011/012/butler_task 既有持久化层；本需求是统一展示 + filter chip。

---
实现笔记：
- 新建 `src-tauri/src/panel_reports.rs`：`ReportSource` enum (Scheduled / Deferred / ButlerArchive) + `PanelReportEntry`。数据 join 三步：butler_history.log 给元信息 (ts / action / title / spec excerpt) → scheduled_reports.json + deferred_tasks.json 按 id 拿完整 spec（命中时） → speech_history.log 按 ts ±5s 拿完整 output。零新文件。两 Tauri 命令 `panel_reports_list(filter, include_archived, limit)` + `panel_reports_get(id)`。
- 前端 `src/components/panel/PanelReports.tsx`：FILTER_CHIPS 四档 (all / scheduled / deferred / butler_archive) + 归档 toggle + 单条 expand。`attachmentCache` 思路镜像 009 的缓存？不用 —— output 是字符串，直接 invoke 拿到 entries 后渲染。
- PanelApp.tsx：TABS 加 "回查"，挂载 `<PanelReports />`。
- TG：新 `ReportsList { n }` / `Report { id }` variants（与 011 的 `/reports` `/report_del` 是**配置**namespace，本命令是**执行**namespace，两者互补）。bot.rs handler 调相同后端，输出风格平行。
- 缺口：「工具调用链折叠 chip」未做。output text 是 LLM 最终回复，没有 tool-call 结构化痕迹可解；要做需在 record_speech 同步落 tool_calls JSON sidecar。本轮先确保「列表 + 展开 + filter + TG 对偶」管道闭合；tool chain 折叠留给后续单独需求。
