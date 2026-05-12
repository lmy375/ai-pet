# 设置页 config 导出 / 导入快照

## 需求

换电脑 / 在第二台机器复用同一份配置时，目前要手动复制 config.yaml + SOUL.md
两个文件。一键打包成可粘贴的字符串就够大多数场景（不上云，符合 GOAL 的约束）。

## 格式

```json
{ "version": 1, "config_yaml": "...", "soul": "..." }
```

外层 base64 编码 → 一行可贴的字符串（典型大小 ~4KB）。version 字段给将来 schema
演进留口子（导入时校验 == 1，否则拒）。

## 实现

### 后端

`src-tauri/Cargo.toml` 加 `base64 = "0.22"`（项目已有 transitive 但显式声明
便宜，避免间接版本飘移）。

`src-tauri/src/commands/settings.rs`：

- `struct SettingsSnapshot { version, config_yaml, soul }`
- `export_settings_snapshot() -> Result<String, String>`：读 config_raw + soul →
  JSON → base64
- `import_settings_snapshot(payload) -> Result<(), String>`：trim → base64 解
  码 → UTF-8 → JSON → 校验 version + serde_yaml 能 parse 成 AppSettings 后才
  落盘；任一步失败原样回错给前端
- 写盘前确保 parent dir 存在（首次导入到全新机器场景）

`lib.rs` 注册两个命令。

### 前端

`src/components/panel/PanelSettings.tsx`：

- `handleExportSnapshot`：invoke 拿字符串 → writeText 剪贴板 → message 显字符数
- `handleImportSnapshot`：armed 二次确认（与 reset 同模式）。第一次点：读剪
  贴板 + 简单预检（trim 非空）+ 提示"再点一次确认"；5s 内再点：调
  import_settings_snapshot + 刷 form / soul / rawYaml
- Save 按钮区加两个新按钮"导出快照 / 导入快照"，与"重置默认"并列；
  导入 armed 态走 reset 同款红填充

## 安全考虑

base64 是编码不是加密 —— payload 含 api_key 明文。新建的 TODO #5 已记：
"导出后剪贴板有 secret"提示。这一条作为 follow-up 加红字 banner。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 点导出快照 → message"已复制 snapshot（X 字符）" → 任何 text editor 粘出可读 base64
  - 改 config 后点导入快照 → 第一次提示"再点一次确认（5s 内）" → 再点 → form 字段 + SOUL 全恢复 → message"已导入 snapshot"
  - 剪贴板空 / 非 base64 → 红 message 显具体错误
  - 旧版 snapshot（version!=1）→ 拒导入并提示版本不被支持
  - 损坏的 config_yaml → 拒导入 + 显 serde_yaml 错误

## TODO 池清空 → 自主提案

按规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. PanelChat session 全部打包成快照
2. PanelTasks 历史归档导出按日期分组 markdown
3. /image -s 参数单次覆盖 size
4. ChatMini Esc 行为补完 + help 速查更新
5. 导出快照后 "含 API key" 红字提醒
