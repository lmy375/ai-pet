# 043 · user 人物关系图谱 — "了解用户"的核心维度真空白

memory item 是事件式（一件事一条），027 topic arc 是主题式（关心什么），006 / 019 / 040 是 user 自己的画像（怎么说话 / 沟通偏好 / 关心动作）。但 user 反复提到的「我妈 / 我朋友小张 / 我老板 K / 我女朋友」这些**生活中的关键人物**完全没有结构化承接 — pet 每次提及都从零开始重新理解。GOAL「了解用户」最具体的"你身边都有谁"维度真空白。

需求：
- 新 store `user_people`，每条 entry：name / alias 列表、与 user 关系（亲属 / 朋友 / 同事 / 伴侣 / 其它）、首次提及 ts、提及次数、累积 attributes（pet 学到的"这个人是个 ___ 的"）。
- 写入路径：user turn 中 LLM 检测关系性引用（"我妈"、"老板 K"、"小张"、"我女朋友"等）+ 同一指代累计出现 ≥ 3 次 → 自动落或更新 user_people。
- attributes 累积：每次提到该人 pet 提炼一句新观察（"老板 K 总是周五发紧急活儿"、"妈喜欢清淡饮食"）+ 旧 attributes EWMA 衰减，避免无限堆积。
- 所有 LLM prompt 头部注入 user_people 摘要（按提及频次 top-5）；pet 后续提及这些人时上下文自带。
- TG `/people` 查看 + `/people_clear <id>` 撤回错识 + `/people_pin <id>` 锁定不被自动改名 + `/people_merge <id1> <id2>` 合并同一人不同 alias。
- 023 session_distill 命中含人物提及的 distill item 自动 link 到对应 user_people entry，方便回访。

---
实现笔记：
- 新建 `src-tauri/src/user_people.rs`：`PersonEntry {id, name, aliases, relation, first_seen_at, last_mention_at, mention_count, attributes, pinned, cleared, cleared_at}` + `Relation {Family, Friend, Colleague, Partner, Other}` 中英 parse + icon。常量集中 `MENTION_THRESHOLD=3` / `MAX_ATTRIBUTES=5` / `INJECT_TOP_N=5`。
- 操作：`mention_person`（name 已存在 → count++ + 新别称自动加 alias；relation 首次落，后续不覆盖避 LLM 不稳定推断扰动）/ `add_attribute`（FIFO cap 5 + 重复跳过避堆积）/ `merge_persons`（src→dst 合并 count+alias+attr，src 软删；pinned src 拒）/ `clear_person` / `set_pin`。
- Pure: `find_by_name`(case-insensitive name+alias，cleared 不参与) / `stable_people_desc`(active + count ≥ 3 by count desc) / `format_for_inject`(top 5 + 反指令禁每轮罗列 / 不相关话题硬扯) / `format_for_listing`(active first + cleared 末尾)。
- 9 单测：relation 中英 parse + label 协议 / find_by_name alias case-insensitive + cleared 跳 / stable threshold + cleared 跳 / inject 空 + 含 icon + 反指令 / listing active-first 排序。
- 新建 `src-tauri/src/tools/person_edit_tool.rs`：LLM tool action `mention` / `add_attribute`；`merge / clear / pin` 故意不开 LLM 入口（避免 LLM 误判合并造成数据丢失，显式 user TG 命令）。工具描述含「不要为 generic pronouns 调用 / 一轮最多 1 attribute / cap 5 FIFO」反指令。
- 集成 hooks：
  - inject 站点：与 037 goals 同 11 chat pipeline 站点（replace_all 8 + 3 单独 Edit），紧跟 goals 之后
  - 4 Tauri 命令 + 1 LLM tool 注册
  - TG 四件套：`/people`（含 ✅/📌/🚫 chip + icon + alias + attr）/ `/people_clear <id>` / `/people_pin <id>` / `/people_merge <dst> <src>`
- **缺口**：
  1. **023 session_distill 自动 link**：spec「distill item 含人物提及自动 link」未做。需在 023 LLM write distill item 路径加 hook 扫 stable_people → 加 `[link_people: id1,id2]` marker。`find_by_name` 已 pub 备用，留下次单独刀。
  2. **EWMA attribute 衰减**：spec 写「累积 attributes ... 旧 attributes EWMA 衰减」。本 v1 简化为 cap+FIFO（超 5 剔最旧）；EWMA score+衰减需 ts-weighted ranking，留 follow-up。
  3. **mention_count 不衰减**：count 只增不减，长期老 entry 累积过高分。可加 monthly normalize（27 topic_arc 同问题）。
  4. **relation 首次落不可改**：LLM 第一次推断错时无 TG 入口改 relation；可后续加 `/people_set_relation`。
  5. **Tauri 命令暂无 Panel 前端使用**：暴露给将来 PanelPeople 备用。
