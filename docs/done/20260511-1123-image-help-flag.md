# /image -h help 文案

## 需求

`/image` 已经长了三个 flag（-n / -r / 历史模糊匹配），用户得读 README 或翻
done docs 才能知道。在命令本身加 `-h / --help` 暴露内置 help 文案，省外查
context。

## 实现

`src/components/panel/slashCommands.ts`：

- SlashAction 加 `kind: "imageHelp"` 分支
- parser 顶部加 `/^(?:-h|--help)(?:\s|$)/i` 早出守门 —— 命中即返
  `{ kind: "imageHelp" }`，跳过后续 -n / -r 解析
- 新 `formatImageHelpText()` 纯函数返回 multiline 文案，列：单图 / -n N / -r /
  -r alone / 组合 flag 例子；底部提示历史菜单交互

`src/components/panel/PanelChat.tsx`：

- 新 switch case `imageHelp` → `pushLocalAssistantNote(formatImageHelpText())`
- 排除 `imageHelp` 不进 slash 使用频次记录（不是 SLASH_COMMANDS 列表里的真
  命令，记了也排不进 menu）

`SLASH_COMMANDS` description 也更新提示 `-h help` 入口存在。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - `/image -h` → assistant 行显内置 help（emoji 标题 + flag 列表 + 例子）
  - `/image --help` → 同
  - `/image -h dragon` → 仍走 help 路径（早出在 prompt 解析前）
  - `/image dragon -h` → -h 不在 arg 头 → 走普通 prompt 路径（`-h` 当 prompt 一部分）
  - help 输出不调后端 image_generate，不耗 API quota
  - 命令面板 `/im` 时菜单 description 显新文案

## 不在本轮范围

- 没把 image_size 字段 / image_model 设置项的描述也塞进 help —— 保持 help
  聚焦"命令本身的 flag"；设置项是 settings 关注
- 没做"/help image" 子命令分流 —— 单独 `-h` 已足够直观，不需要额外语法
