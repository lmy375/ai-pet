# PanelTasks "🗑️ 清结束" 按钮

## 需求

长期使用后 butler_tasks 里的 done / cancelled 任务累积成几百条 ——
PanelTasks 默认隐藏（showFinished=false）但仍占用 memory index 文件
大小 + 增加 LLM prompt 中 butler_tasks_hint 的解析成本。补一键
bulk delete 全部已结束任务的按钮，armed 二次确认防误触。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 `finishedTaskCount` useMemo：`tasks.filter(t => done || cancelled).length`
- 新 state `clearFinishedArmed` + `clearFinishedBusy`，与 iter #202
  reset-stash 同确认模式（首点变红 + 3s revert，busy 期间 disabled）
- 新 `handleClearAllFinished` useCallback：
  - armed gate：首点 set armed + 3s 自动 revert
  - 真触发：collect targets → 逐条 `invoke("memory_edit", {action: "delete",
    category: "butler_tasks", title})`
  - 单条失败不 abort，统计 ok/fail，反馈"成功 N · 失败 M"
  - 完成 → reload → 队列刷新
- 在已有 dueChip / errorTaskCount chip 同行追加 chip "🗑️ 清结束 (N)"：
  - 仅 `finishedTaskCount > 0` 时浮（无可清不显示）
  - default 灰底 / armed 红边红字 / busy slate gray
  - 复用 bulkResultMsg 4s toast

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 全无 done / cancelled → chip 不显
  - 队列含 done / cancelled → chip 显 "🗑️ 清结束 (N)"
  - 点击 → 变红 "再点确认 (3s)"
  - 3s 内再点 → 变灰 "清除中…" → 逐条删 → "已清除 N 条已结束任务" toast
  - 3s 不点 → armed revert，无副作用
  - 部分删除失败 → "清除完成：成功 N · 失败 M" toast
  - reload 后队列刷新，showFinished=true 视图下也立即没掉

## 不在本轮范围

- 没做"按日期范围清"（如仅清 30 天前）：先做全量 bulk；范围筛选可
  后续配合 due 字段加 UI
- 没做"导出后再删"（先备份 markdown）：现"全部导出 MD" / 单条复制为
  Markdown 等路径用户可自行做；不在此 button 范围
- 没做并发 invoke（一次性 Promise.all）：内存任务通常 < 100 条，逐
  条调可控；并发可能撞 fs lock 风险
- 没做 progress bar / 中途取消：< 100 条 IO 通常 < 1s 不需中断；> 1000
  条等用户实际反馈再加

## TODO 池剩余

空。下一轮需自主提需求。
