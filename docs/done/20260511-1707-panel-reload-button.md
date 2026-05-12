# 设置页 reload 当前 panel 按钮

## 需求

🔄 重启 pet 窗口只重建 main webview。但用户改了纯 panel 内字段（mcp / chat /
proactive 等）想看新值生效时，不需要重启 pet，只需让 panel JS 重读。加 🔁
reload 此面板按钮直接 `window.location.reload()`。

## 实现

`src/components/panel/PanelSettings.tsx` 重启 pet 按钮旁加 button：

- onClick: `window.location.reload()`
- 无 armed 二次确认 —— reload 比重启窗口便宜得多，可逆性高（重新拉 settings
  自动恢复，不影响 native window 状态）
- tooltip 提醒"未保存的 form 草稿会丢，先点最下方『保存』"
- 视觉与重启按钮区分（保持灰，不变红 armed 态）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 改了 mcp server 列表 → 保存 → 点 🔁 reload 此面板 → panel JS 重新拉
    settings + 重连 mcp 等
  - pet 窗口 / debug 窗口不动
  - 没保存的草稿丢失（与文档一致）

## 不在本轮范围

- 没做"reload 前自动 save"：用户主动控制保存时机更稳；tooltip 已提醒
- 没做"reload 后保留 active tab"：panel 默认回首 tab，多数场景用户切回设置页
  就行；要保留得用 sessionStorage 缓存 activeTab

## TODO 池剩余

- PanelTasks 任务行右键菜单
- PanelChat 跨会话搜索 hit 高亮
