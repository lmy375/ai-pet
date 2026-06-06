# 056 · PanelMemory 全文搜索

搜索 UI 已存在（input + ⌘F + Enter），匹配只看 item title/description。

- ✅ part1：`memory_search` 加 cat_name + cat label 匹配（搜「butler」/
  「管家」命中整 cat）；抽 `item_matches_query` 纯 helper + 6 测试 pin；
  PanelMemory placeholder 改示例语料。1895 tests + tsc clean。
- ❌ part2 拒绝：≥3 字语义匹配 + 「精确/语义」chip 区分与
  `feedback_pet_core_5_no_meta_no_gamification` 冲突 —— chip 区分模式
  正是该规则禁止的 audit-chip-on-main-view。TG /recall 已通过 038
  提供语义入口；面板侧 substring 已够。
