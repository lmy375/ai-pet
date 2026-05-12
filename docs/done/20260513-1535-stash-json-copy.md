# PanelDebug "📥 stash JSON" 按钮

## 需求

调 prompt / 复现 bug 时常需把 in-process stash 全部抓出来给 LLM / 同
事看。已有的多个命令（get_last_proactive_prompt / reply / meta /
get_recent_proactive_turns / get_last_manual_fire / get_manual_fire_history）
得手动一个个调。补一键 JSON 抓取按钮。

## 实现

`src/components/panel/PanelDebug.tsx` 在 "🔄 reload" 按钮后插入 "📥 stash
JSON" 按钮：

- onClick：Promise.all 并发 6 个 invoke 调用
- 组合成单对象 `{exported_at, last_proactive_prompt, last_proactive_reply,
  last_proactive_meta, recent_proactive_turns, last_manual_fire,
  manual_fire_history}`
- JSON.stringify (indent 2) → writeText
- 成功 / 失败都通过 proactiveStatus toast 反馈
- accent 紫底白字（与其它 toolbar 按钮区分开 — 这是"调试 dump"语义）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 进程刚启动无 fire → JSON 中各字段为 empty string / null / []
  - fire 一次后 → prompt / reply / meta / turns / last_manual_fire 都
    有值，manual_fire_history 长度 1
  - 多次 fire 后 → manual_fire_history 累积，cap 5
  - 粘到 JSON parser 验证结构完整 / Indent 2 易读
  - 复制成功后 toast 显字符数

## 不在本轮范围

- 没把 settings 也一并包入：iter #215 已有 PanelSettings "📋 导出 md"
  + iter #207 PanelDebug "issue 模板" 覆盖含 settings 路径；这条聚焦 stash
- 没做"导出到文件"对话框：剪贴板已是最快路径
- 没在 JSON 加 schema 注释：JSON.stringify 不允许；用户阅读 indent
  2 + 字段名已够
- 没让按钮支持"近 N 分钟过滤"：进程内 stash 本就 bounded（ring cap 5），
  不需细粒度

## TODO 池剩余

- PanelTasks priority badge 右键菜单
