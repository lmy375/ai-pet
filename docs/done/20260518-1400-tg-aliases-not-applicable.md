# TG `/aliases <title>` — 范围外 pivot drop（iter #567）

## Discovery

TODO 提案：「TG `/aliases <title>`：列本 task description 历史 rename
痕迹（butler_history 扫 create+rename event）—『这条曾叫什么』audit」。

**实际**：`memory_rename` (commands/memory.rs:690) 不写 butler_history
事件。butler_history.log 仅由 `butler_task_edit` tool（memory_tools.rs:
323）记录 `update` / `delete` action，加 consolidate.rs 的 `delete` /
`archive`。rename 操作完全未在历史里留痕：

```rust
// memory_rename() at line 690 - 改 index + 移文件 + SQLite mirror
// 但不调 butler_history::record_event(...)
item.title = new_trimmed.clone();
item.detail_path = new_detail_path.clone();
item.updated_at = now_iso();
// ... mirror_butler_rename / mirror_todo_rename / 等
// 不写 butler_history.log
```

## 现有可用历史源 vs 需求

butler_history.log 行格式：`<ts> <action> <title> :: <desc-snippet>`

- 当前 actions：`update`, `delete`, `archive`, `create`
- 缺：`rename`

可能的 inference 路径（都不可靠）：

1. **跨 ts 用 description 相似度匹配**：对同一 owner 修改痕迹排序，看
   description 段几乎不变但 title 变 → 推测是 rename。但 description
   80 字截断 + 同时改 title 和 desc 时不可分辨
2. **「memory_not_found 后跟 memory_create 同 desc」启发**：rename 在
   index 层级是 move，不会触发 not_found；这个启发对显式 rename 无效
3. **`updated_at` 锚点 + SQLite mirror 反查**：mirror_butler_rename 写
   SQL 但不留 hist；查不到 old_title 的源

## 真要实现的工作量（>= 2 iters）

1. **Backend：rename event 写 butler_history**
   - memory_rename 加 `record_event("rename", new_title, &format!("[was: {}]", old_title))` 调用
   - 注意 80 字截断会让长 old_title 被砍；考虑超出长度时截 old_title 而非 desc
   - 同样改 memory_cascade_rename_in_detail_md 的入口
2. **TG `/aliases <title>` 6-surface 同步**：Enum / parser / handler /
   tests / 三 drift list / help-table en+zh + help-detail
3. **Handler 双向 scan**：title 在 rename event 里既可能是 new（直接
   match）也可能是 old（在 desc `[was: ...]` 里 match）— 双向递归
   reconstruct alias chain
4. **测试**：current title 在最后 / 跨多 rename / 同 old 重命名又改回
   等 edge cases

跨多层 + 含 rename chain reconstruction 不平凡。但**已有历史不可追溯**
（pre-本 lift 的 rename 都不可见）— 即使做完，对老 task 仍无 alias 数
据。本特性 ROI 偏低。

## 现有 alternative

owner 想看「这条 task 曾叫什么」实用路径：

- **手工 `/timeline <title>`**：扫 butler_history 看全谱 event — rename
  在那里看不到，但 update 的 desc 变化能侧面看出。粗略 audit 但缺 alias
- **git history of `~/.config/pet/memories/index.yaml`**：若 owner 启
  git track 该目录（多数没），可 `git log -p index.yaml` 看 title 变
  化。脱离 pet 内部入口
- **owner 自己记**：rename 是低频操作（多数 task 一辈子不改名）— owner
  能记住「我之前叫它 X」的情况，需 audit 工具的 ROI 不高

## Decision

**不实现 /aliases**。三条理由：

1. 数据 substrate 缺失（memory_rename 历史无 rename event 记录）—
   需 backend lift 写新 action type + 6-surface frontend
2. 历史不可追溯：即使 lift 完，pre-lift rename 完全不可见
3. rename 是低频操作 — alias chain audit 的实用需求度不如其它 7d audit
   类需求（/cat_growth_7d / /pin_grow_7d / /pinned_drop_7d 等系列）

procedure 教训：propose 「history audit」类需求时，先 grep 该操作的
event-recording 路径 — 不是所有 action 都进 history.log。本 iter 是
第 4 次踩同类「data substrate vs feature intent」差（iter #554 / #560
/ #565 同模式重复）。可以试着在 README / GOAL.md 加一句 "history-
based audit feature 在 propose 前先确认 backend 事件 logging
coverage"。

## Future iters (out of scope)

- **lift rename event to butler_history**：先做 backend 单步（memory_
  rename 调 record_event("rename", ..., "[was: ...]")) — 让未来 rename
  可追溯。再做 frontend /aliases 命令。两 iter 拆分
- **「git track ~/.config/pet」用户教育**：README 加段子建议 owner
  把 memories dir 拉 git — 给非 audit 也是 disaster recovery 保险
- **/timeline 内显 rename**：未来 lift 完 rename event 后，/timeline
  自动含 「✏️ MM-DD HH:MM · 重命名 from 「<old>」」行
