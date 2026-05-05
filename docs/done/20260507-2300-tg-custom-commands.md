# TG bot 自定义命令矩阵 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot 命令矩阵编辑入口：`tg_command_registry` 现在硬编码 5 条；让用户在 settings 加自定义命令名 (LLM 工具映射)，让"快捷调任意 LLM 工具"成立。

## 目标

`tg_command_registry()` 硬编码 5 条命令。当 LLM 工具不断扩充，用户希望
打 `/timer 5min` / `/translate 你好` 这种快捷命令时，TG 客户端补全表里
没有；只能裸文本。本轮加 settings 字段 `telegram.custom_commands`：用户
列出自定义命令名 + 描述 → bot 启动时一并注册到 TG 客户端补全表 → 用户
在 TG 输 `/` 时看到这些自定义命令；调用时**当作普通文本走 chat pipeline**
（LLM 自由 dispatch 工具），不绑定具体 tool。

## 非目标

- 不绑定"命令 → 特定 tool" 直接映射 —— LLM 在 chat pipeline 已能根据
  上下文选合适的 tool；强行 1:1 映射会让用户得为每个 tool 都设条命令，
  不如让 LLM 当中介。
- 不做命令模板（占位 / 默认参数）—— 文本即需求，"/timer 5min" 文字本身
  就是 LLM 的 prompt；模板增加心智成本，价值低。
- 不在 TG 端开管理命令（如 `/addcmd`）—— 桌面 settings 是合适入口；管
  理命令绕一圈反而易冲突。

## 设计

### 数据结构

`TelegramConfig` 加 `custom_commands: Vec<TgCustomCommand>` (default 空)。

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TgCustomCommand {
    pub name: String,          // 不带 `/`，lowercase ASCII
    pub description: String,   // ≤ 256 字
}
```

### registry 合并

```rust
pub fn merged_command_registry(
    custom: &[TgCustomCommand],
) -> Vec<(String, String)>;
```

返回 hardcoded 5 条 + 过滤后的 custom 条目。过滤规则：
- `name` 非空 + lowercase ASCII / `_` / `0-9`（与 TG API 约束一致）
- 不与 hardcoded 名重复（避免覆盖既有命令语义）
- 同 custom 列表内重名也去重（保留首个）
- `description` trim 后非空且 ≤ 256 字

无效条目静默丢弃 —— 避免一条配错就 bot 起不来。

### bot 启动注册

bot.rs `set_my_commands` 调用点改为读 settings.telegram.custom_commands
传给 `merged_command_registry`。

### 调用分发

bot.rs `handle_message` 在调 `parse_tg_command` 之前先看 text 是否
`/{custom_name} ...`：
- 命中 custom name → **不**走 command pipeline，直接 fall through 到
  chat pipeline（让 LLM 当普通文本看待 + 自由选 tool）
- 不命中 → 走原 parse_tg_command 路径

为何先做 custom 拦截：避免 custom name 撞到 typo-suggest 触发
"你是不是想发 /xxx 吗？" 这种误导。

`HandlerState` 加 `custom_command_names: Vec<String>` 字段（启动时填充
小写化 + 校验过的 name 列表，O(1)..N 量级查询直接 linear scan）。

### 前端 settings UI

在既有 TG section 下方加"自定义命令" 子段：
- 一个 textarea，每行 `name: description` 格式
- 保存时 parse 成 Vec<TgCustomCommand>，写到 settings
- 解析失败行（缺 `:` / name 非法 / description 空）静默丢弃，前端 hint
  显示 N 条解析成功

textarea 比 add/remove 行 grid 简单，编辑批量条目快；解析失败 silent
drop 让用户自由实验，不被严格 form validation 卡。

### 测试

后端：
- `merged_command_registry`: 空 custom → 等于 hardcoded
- 加 1 条合法 custom → 总数 hardcoded.len() + 1
- name 撞 hardcoded → 丢弃
- description 空 → 丢弃
- name 非法字符 → 丢弃
- 多条同名 custom → 保留首个
- description 超 256 → 丢弃

前端 settings 解析：
- 简单 split + trim 行内联，前端无 vitest，靠 tsc + 手测

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | TgCustomCommand struct + TelegramConfig 字段 |
| **M2** | merged_command_registry + 单测 |
| **M3** | bot.rs 启动用 merged registry + HandlerState 携 custom names + handle_message custom 拦截 |
| **M4** | 前端 settings textarea UI + parse + 保存 |
| **M5** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `tg_command_registry` (hardcoded 部分)
- 既有 set_my_commands 启动路径
- 既有 settings 序列化 / 持久化

## 进度日志

- 2026-05-07 23:00 — 创建本文档；准备 M1。
- 2026-05-07 23:10 — M1 完成。`TgCustomCommand` struct + `TelegramConfig.custom_commands: Vec<...>` 字段 default 空；前端 useSettings 的 TelegramConfig 同步 + 默认 [] 兜底。
- 2026-05-07 23:20 — M2 完成。`merged_command_registry(custom)` 纯函数：先 hardcoded 后 custom，规则丢弃空 / 撞名 / 非法字符 / 描述空 / 描述超 256 / 同名 dedup 保首；5 个新单测覆盖空合并 / append / 各种 invalid drop / dedup / desc 越限。
- 2026-05-07 23:30 — M3 完成。bot.rs `set_my_commands` 改用 merged_command_registry；HandlerState 加 `custom_command_names` 字段（启动时基于 merged 减去 hardcoded 拿剩余）；handle_message 在 parse_tg_command 前 short-circuit：text 以 `/{custom_name}` 开头 → 不走 dispatch，fall through chat pipeline 让 LLM 自由处理。
- 2026-05-07 23:35 — M4 完成。PanelSettings TG section 加 textarea：每行 `name: description`，前端 silent drop invalid 行；保存即写 settings；提示文案说明改完需点 "保存并连接" 才生效（bot 启动期一次性读取）。
- 2026-05-07 23:40 — M5 完成。`cargo build` 14.15s 通过；`cargo test --lib` 974 通过（+5 新测）；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
