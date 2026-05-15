# 聊天 `/whoami` 命令 — 宠物自我介绍

## 背景

TODO 第一项（auto-proposed 本轮）：

> 聊天 `/whoami` 命令：宠物自我介绍 —— 陪伴天数 + 当前心情 + 自我画像首段 + 近常用工具 top 3，一行清单"我是谁、现在感觉如何、最近在练什么手艺"。

「人格」tab 已经聚合了陪伴 / 心情 / 自我画像 / 常用工具四块自我信号，但要切到面板才能看；很多用户的实际 attention 在聊天框里。`/whoami` 把这四源在 chat 里聚合一次输出，体感像 IM 朋友自报家门（"我叫 X，跟你认识 Y 天，现在心情 Z，最近在练 K"），与已有的 `/mood` / `/version` slash 视觉一致。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 列表加 `{ name: "whoami", description: "宠物自我介绍：陪伴 / 心情 / 自我画像 / 近常用工具", parametric: false }`，放在 `/mood` 旁边（语义相邻）。
- `SlashAction` 加 `{ kind: "whoami" }`。
- `parseSlashCommand` switch 加 `case "whoami": return { kind: "whoami" }`。

### `src/components/panel/PanelChat.tsx`

`executeSlash` switch 新增 `case "whoami"`：并发 fetch 4 个 IPC 源，每个独立 `.catch` 兜底（防一处挂导致整段不渲染），最后排版 multi-line system-note bubble。

数据源：

| Source | IPC | Fallback |
|---|---|---|
| 用户称呼 | `get_user_name` → string | `""`（不渲染该行）|
| 陪伴天数 | `get_companionship_days` → number | `null`（不渲染）|
| 当前心情 | `get_current_mood` → { text, motion, raw } | `null`（不渲染）|
| 自我画像 | `get_persona_summary` → { text, updated_at } | `null`（不渲染）|
| 近常用工具 | `get_top_tools_used` → ToolUsageStat[] | `[]`（不渲染）|

输出排版：

```
🪪 /whoami
🐾 我叫你「Moon」。
📅 与你相伴已 14 天。
💗 现在的心情：今天阳光特别足 · 动作组 happy
🪞 自我画像：观察 Moon 工作时段是上午 10-12 / 下午 14-18…
🛠 近常用工具：`shell`×12 · `read_file`×7 · `weather`×3
```

边界处理：

- 自我画像可能很长 → 取首段（按双空行切分的第一段），> 90 字截断 + `…`，单行不至于压扁 chat 列表。
- 心情 `raw === ""` 表示从没记过（与 `/mood` 三态语义一致），不渲染心情行。
- top tools 取 top 3 而非 top 5（`/mood` 等其它命令都是一行清单密度），count 显示让"频次差异"被感知到（用 12 次的工具与用 1 次的不是同一回事）。
- 所有源都空（刚装机 / 全清状态）→ 输出兜底"🐾 还没攒到自我介绍的素材，先一起聊聊吧。"

`Promise.all` 在所有 `.catch` 都返了 fallback 之后 resolve —— 没有真正会 reject 的支路，不需要 `allSettled`。

### `docs/TODO.md`

- 删 `/whoami` 这一行（移到本 done 文件）。
- 剩余 5 条 auto-proposed 新需求：任务模板个性化 / PanelMemory 批量删除 / 任务依赖 / ↑ 召回入编辑 / 任务完成 sparkle。

### `README.md`

§3「自我进化」加亮点行，与既有「技能简档」紧挨着（说同一件事的不同 surface）。

## 不做

- **不让 LLM 调用 `/whoami`**。是用户面向的 slash，不是 tool；后端不需要新增 IPC。
- **不缓存结果**。每次都重新 fetch；信号变化（新 tool 调用 / 心情更新）即时反映。4 个并发 IPC < 50ms 总耗时，不必加缓存层。
- **不写 unit test**。前端无 vitest 配置；执行路径是单个 switch case + 字符串拼装，逻辑明显。
- **不动 TG `/whoami`**。本轮仅桌面端；TG 端等用户表达需求再加（TG 已有 `/mood` 单源命令，可作为 follow-up 的参照模式）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.22s

## 后续

- TG `/whoami` 对偶（与 `/mood` 同模式延伸）。
- 自我画像渲染 markdown（当前是 raw 文本截断；自我画像里偶有 bullet / emphasis）。
- 自我介绍频率：若 `/whoami` 调用频繁出现，可考虑早安简报里改为偶尔自报家门。
