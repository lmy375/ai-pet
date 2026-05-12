# 导出快照后 API key 警告

## 需求

上一轮 `导出快照` 把 config + SOUL 转 base64 写剪贴板，但 base64 是编码不是
加密 —— payload 含 api_key / telegram bot_token 等 secret 明文。用户在 IM /
public issue 顺手贴出来风险大。导出成功时弹一行红字提醒。

## 实现

`src/components/panel/PanelSettings.tsx`：

- 新 state `securityNotice: string` + timer ref
- `handleExportSnapshot` 成功路径写：
  - 原 green message"已复制 snapshot (X 字符) 到剪贴板"
  - 同时 setSecurityNotice("⚠ snapshot 含 API key / Telegram token 明文（base64 只是编码不是加密）—— 贴到 IM / 公开 issue 前请审核。")
  - 8s setTimeout 自清，避免长期占视觉位
- 按钮行下方独立渲一个红色 banner（var(--pet-tint-red-bg / -fg)），仅 securityNotice 非空时显
- 后续点击导出会先 clear timer + setSecurityNotice("") 避免上次未清的 banner 与本次混

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点导出快照 → 顶上绿色"已复制 snapshot..." + 下方红色 banner"⚠ snapshot 含 API key..."
  - 8s 后红 banner 自动消失（绿 message 仍在按原 flow 控制）
  - 8s 内再点导出 → 红 banner 重置 timer + 重显
  - 失败路径不显红 banner（catch 分支不触发 setSecurityNotice）

## 不在本轮范围

- 没在 export 前给"我要不要重置 api_key 字段再导出？"prompt：实操中用户通常
  想"完整带过去"；硬要导脱敏版给一个新 flag，复杂度不匹配收益
- 没把 secret 字段 mask 处理：snapshot 是完整 yaml/markdown 文本，做 mask 又
  违背"完整快照"语义；让用户判断分享场景才是对的
