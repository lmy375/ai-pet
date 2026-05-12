# 临时 prompt fire modal "📥 上次 prompt" 按钮

## 需求

iter #238 的临时 prompt modal 默认填 SOUL.md 内容。但调 prompt 时常
想从"上一次实际发到 LLM 的完整 prompt"（含 butler_tasks_hint / mood /
feedback 等动态拼接段）作为起点 —— SOUL 只是其中一段。补 "📥 上次
prompt" 按钮一键加载。

## 实现

`src/components/panel/PanelDebug.tsx` 临时 prompt modal footer 在
"取消"按钮前插入新按钮：

- onClick async invoke `get_last_proactive_prompt`
- 非空 → setTempPromptDraft 覆盖 textarea（用户原 draft 丢失，可接受
  — 加载是显式动作）
- 空（进程刚启没 fire 过）→ proactiveStatus toast "上次 prompt 为空"
- 调用失败 → silently 留原 draft（与既有 get_last_proactive_prompt
  fallback 一致）
- 视觉：与"取消"同款 ghost button + muted 灰字

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 进程刚启 + 打开 modal → 默认 SOUL 填入
  - 点 "📥 上次 prompt" → toast "上次 prompt 为空"
  - 先 fire 一次 → 再打开 modal → 点 "📥 上次 prompt" → textarea 替
    换为完整动态 prompt
  - 改后 fire → 用 modified prompt 跑（与 #238 路径一致）

## 不在本轮范围

- 没做"上次 reply" 也加载（reply 是 LLM 输出，不该塞进 SOUL）
- 没在 modal 顶部加 segmented "from SOUL / from last prompt" toggle：单
  按钮覆盖 + 默认 SOUL 路径已够；toggle 增 UI complexity
- 没记 "load last prompt 历史"：单次操作没历史价值

## TODO 池剩余

- PanelChat 自定义模板 "🛠 管理" modal
- PanelTasks priority input ▲▼ 微调按钮
- PanelChat marks modal entry "🗑" 移除标记
- PanelMemory butler_tasks "📋 复制完整 prefix + topic"
