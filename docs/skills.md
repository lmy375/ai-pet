# 技能（Agent Skills）

技能是一份写给宠物看的操作手册：一个目录，里面放一份 `SKILL.md`，外加它需要的
参考资料和脚本。所有 Agent 共享同一个技能目录。

想让宠物会做某类专门的事（跑某个内部 CLI、按某套流程处理某类文件），把步骤写成技能，
比塞进 `SOUL.md` / `MEMORY.md` 好——记忆是常驻的，每轮都进上下文；技能是按需加载的。

## 目录结构

```
~/.agents/skills/               ← 技能目录（可在设置里改）
  cobo-agentic-wallet/          ← 目录名 = 技能标识（slug）
    SKILL.md                    ← 必需
    references/*.md             ← 可选：SKILL.md 里按需引用
    scripts/*.sh                ← 可选：宠物用 bash 执行
  my-other-skill/
    SKILL.md
```

没有 `SKILL.md` 的子目录会被忽略，不算错误。

## SKILL.md

以 YAML frontmatter 开头，`name` 和 `description` 两个字段：

```markdown
---
name: cobo-agentic-wallet
description: 用 caw CLI 管理 Cobo 钱包：转账、合约调用、pact 审批。涉及 caw / MPC 钱包 / Cobo 时用。不用于法币支付。
---

## 使用步骤

1. 先读 `references/onboarding.md` 确认环境。
2. ...
```

- `name` 可省，省略时用目录名。
- `description` **必填**——宠物就是靠它判断某个任务该不该用这个技能。写清楚
  「什么时候用、什么时候不用」，比写「这是什么」有用。超过 1024 字符会被截断。
- frontmatter 里的其他字段（`metadata`、`license` 等）会被忽略，不影响解析。
- 正文随便写，宠物会整篇读进去。

## 渐进披露：宠物怎么用技能

每轮对话的系统提示里只放**名称 + 用途 + SKILL.md 的绝对路径**，几十 token。
宠物判断任务命中某条技能时，才用 `read_file` 打开正文；正文里指向的
`references/` / `scripts/` 同样按需打开或执行。

所以技能数量多、正文长都不会撑爆上下文——真正进上下文的只有用得上的那一份。
没有可用技能时，系统提示里不会出现技能这一段。

改完 `SKILL.md` 下一轮立即生效，不用重启。

## 直接调用：`/skill:<名称>`

不想等宠物自己判断，可以点名调用。GUI 聊天框和 pet-cli 里输入 `/` 都会弹出补全：

```
/skill:cobo-agentic-wallet 查一下我主钱包的余额
```

`<名称>` 是**目录名**。命令会展开成一句普通的用户消息发给模型（就是聊天记录里
显示的那句），后面的任务描述可省。

补全列表里每个可用技能各占一行、带用途，所以它同时也是「有哪些技能」的清单，
没有另设一个查看命令。

两个限制：目录名带空格的技能没法用命令调用（仍然能被宠物自己命中）；
`SKILL.md` 解析失败的技能不会出现在补全里，也不会进系统提示——要看它们错在哪，
去设置页的技能卡片，那里会列出来并显示错误。

## 换目录

设置 → 全局 → 技能（Skills）。留空默认 `~/.agents/skills`，卡片上有
`~/.claude/skills`、`<配置目录>/skills` 的一键切换，也可以自己选或手填
（支持开头的 `~`）。对应 `config.yaml` 里的 `skills_dir`。技能是全局的，
`pet-cli` 和 GUI 共用同一个目录。

## 信任模型

技能目录由你自己维护，`SKILL.md` 的内容会被宠物当作指令执行——和 `SOUL.md`
是同一个信任级别。不要放来路不明的技能包。
