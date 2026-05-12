# PanelSettings "📋 导出 md" 按钮

## 需求

既有"导出快照"按钮把 config + SOUL 编 base64 让用户跨机 roundtrip
restore。但当用户想分享 / 提 issue / 给 LLM 自查时，base64 不可读。
补一个 markdown 导出 ——同样的两段数据但包成 fenced code blocks 直接
可读。

## 实现

`src/components/panel/PanelSettings.tsx`：

- 新 `handleExportSettingsMarkdown`：
  - `Promise.all` 并发 invoke `get_config_raw` + `get_soul`（两个早已
    暴露的 tauri command）
  - 拼成 markdown：H1 标题 + 时间戳 + H2 config.yaml fenced (yaml lang) +
    H2 SOUL.md fenced (markdown lang)
  - `navigator.clipboard.writeText(md)` + 成功 toast
  - 与既有 "导出快照"共用 8s security warning 通道（也含 api_key 明文，
    导出前要审核）
- 在 "导出快照" 按钮后插同款 ghost-button "📋 导出 md"
- title tooltip 解释与 snapshot 的差异（人 / LLM 可读 vs roundtrip 用）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 点 "📋 导出 md" → 剪贴板装 markdown 全文 + "已复制 settings markdown
    （N 字符）" toast
  - 紧接浮 8s 红字 "⚠ 含 api_key 等明文，公开前审核"
  - 粘到 GitHub issue / Notion / Discord → 渲染清晰的 H1/H2 结构 + 两
    个 fenced code 段
  - 与"导出快照"两个按钮并存，分别覆盖人读 / 机器 roundtrip 两条路径

## 不在本轮范围

- 没做"自动 redact api_key / token"：用户场景不同（有时分享需保留以让
  maintainer 复现），手动审核更稳；redaction 加 toggle 是 follow-up
- 没集成 PanelDebug issue 模板（与 iter #207）：两条 issue 模板可叠
  加使用（先复制 settings md → 复制 prompt + debug snapshot），两者
  独立保留入口的视觉对应不同 panel 的语义边界
- 没做"导出到文件"对话框：剪贴板已是最快路径；文件保存还要 dialog +
  filesystem 权限

## TODO 池剩余

- PanelPersona "重置 SOUL.md 为内置默认" 按钮
