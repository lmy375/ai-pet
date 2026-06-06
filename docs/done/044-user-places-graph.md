# 044 · user 地点图谱 — 043 的 location 对偶

043 把 user 生活中的人物结构化。但 user 反复提到的「老地方 / X 餐厅 / Y 区 / 我们公司 / 我家附近的 Z」这些**地理 reference** 同样无结构化承接 — user 说"老地方见？" pet 答不上是哪；"明天去 X 餐厅" pet 总要追问"X 在哪、什么类型"。GOAL「了解用户」的另一具体维度真空白。

需求：
- 新 store `user_places`，每条 entry：name / alias 列表、类型（餐厅 / 商店 / 办公场所 / 居所 / 城市区域 / 其它）、首次提及 ts、提及次数、累积 attributes（pet 学到的"这地方 ___"）、可选 lat/lng（来自 user 显式 / 链接抓取）。
- 写入路径：user turn 中 LLM 检测地点 reference + 同一指代累计 ≥ 2 次 → 自动落或更新（地点比人物提及阈值更低，因为「老地方」类高频但短期）。
- attributes 累积同 043 模式（EWMA 衰减），不同的是支持空间属性（"这里离 user 家近"、"X 餐厅是 user 周末常去的"）。
- 所有 LLM prompt 头部注入 user_places 摘要（按提及频次 top-5）；pet 后续提及这些地点不再追问基础信息。
- TG `/places` 查看 + `/place_clear` / `/place_pin` / `/place_merge`（同 043 命令族）。
- 023 session_distill 含地点提及的 distill item 自动 link 到对应 user_places entry。
- 与 041 calendar conflict 协同：日历事件 location 字段反查 user_places 自动 enrich 已知地点。

---
实现笔记：
- 新建 `src-tauri/src/user_places.rs` 镜像 043 user_people 范式（同 store/CRUD/merge/inject/listing 形态），关键调整：
  - `MENTION_THRESHOLD = 2`（spec：地点比人物阈值更低，「老地方」类高频但短期）
  - `PlaceType {Restaurant, Shop, Workplace, Home, Region, Other}` 中英 parse + emoji icon (🍜/🛒/🏢/🏠/🗺/📍)
  - 字段新增可选 `lat: Option<f64>` / `lng: Option<f64>`（v1 仅占位，user 显式 set / 链接抓取后填入，主动 geocode 不在本 v1；merge 时 dst 缺则从 src 借）
  - inject 反指令换语义：「提及这些地点（含老地方/别称）时**不要追问基础信息**（位置/类型）」（spec 痛点对应）
- 新建 `src-tauri/src/tools/place_edit_tool.rs`：action `mention` / `add_attribute`，`merge/clear/pin` 故意不开 LLM 入口。工具描述强调阈值 2 与 043 阈值 3 区别 + 「不要为 generic 空间词调用」反指令。
- 集成：与 043 user_people 同 11 chat pipeline 站点（replace_all 8 + 3 单独 Edit），紧跟 user_people 之后。4 Tauri 命令 + LLM tool 注册。TG quad `/places /place_clear /place_pin /place_merge`。
- 9 单测：place_type 中英 parse + label 协议 / find_by_name alias case-insensitive + cleared 跳 / stable 阈值 2 / 跳 cleared / inject 空 + 反指令 / listing active-first 排序。
- **缺口**：
  1. **041 calendar enrich 协同**：spec「日历事件 location 字段反查 user_places 自动 enrich 已知地点」未做。需在 041 inject_calendar_conflict_layer 增一句「location 命中 user_places 时带入 attribute」prompt 改造，或更深 backend hook。`find_by_name` 已 pub 备用。
  2. **023 distill 自动 link**：与 043 同样的 gap。
  3. **EWMA 衰减**：同 043，v1 简化为 cap+FIFO。
  4. **主动 geocode**：spec 提及「可选 lat/lng（来自 user 显式 / 链接抓取）」；当下 LLM tool 不接受 lat/lng 入参（避免乱猜坐标），需后续从 URL 抓取 / 用户显式输入接入。Tauri 层目前不暴露 set_lat_lng 命令——前端 PanelPlaces 时再补。
  5. **PlaceType 与 PersonRelation 重复字段架构**：两 store 几乎对称（除 lat/lng）。后续若加更多 entity（如 user_objects），可抽 generic store；当下保持 verbose 让 audit 更直观。
