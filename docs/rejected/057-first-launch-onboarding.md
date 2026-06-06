# 057 · 首次启动 onboarding — 让 pet 自我介绍而非 dashboard 砸脸

当前首次启动直接进 dashboard-y UI（截图显示 10 tab + 多 panel），新用户面对陌生 pet figure 不知道 pet 是谁、能做什么、自己应该怎么开始。GOAL「美观可爱 + 情绪价值」第一印象就被破坏 — 典型流失场景。

需求：
- 启动时检测 settings store 是否首次（无 pet name / mood_history 完全为空）→ 覆盖 ChatMini 之上启动 onboarding flow（单页 slide，非多步 wizard）。
- 流程内容（顺序展开 / 不能跳到下一步直到本步完成或显式 skip）：
  1. pet figure 出场 + 一句自我介绍「我是你的小宠物～从今天起就在这里啦」
  2. 引导取名（055）—— 默认显示输入框 + 「就叫'小宠物'吧」一键 fallback
  3. 询问当前心情（落 mood_history 初值，单选 emoji 候选）
  4. 三个示范用法 chip 可点："帮我明早提醒喝水" / "记下我妈生日 X 月 X 日" / "查这个 URL 讲了什么"，点击直接发对话演示一次
  5. 完成 → close flow，转入正常 ChatMini
- 整个 flow 始终保留右上角 ✕ skip；skip 后 pet 自然继续运行，未填项保持空（不强制 placeholder）。
- 不带任何 progress bar / step indicator（保持轻量）。
- 首次完成后写入 settings `onboarded_at`，永不重弹；user 在 PanelSettings 可手动 reset 重新走一次（debug + 演示用）。

---
## ❌ Rejected · 2026-05-24

5 步骤每步都与既有能力重复或弱化既有 primitive，建一个专用 overlay 收益有限：
- 步骤 1（pet 自我介绍）：第一次 proactive turn 即可承担，已经有 morning_briefing / welcome_back / 首启动 prompt 路径
- 步骤 2（取名）：已在 PanelSettings · 055
- 步骤 3（mood）：proactive 已会 ask；mood_history 初值不需要强采集
- 步骤 4（示范 chip）：ChatMini 已有 suggested replies / 输入提示位
- 步骤 5：自动 close 不算交互内容

实际「10 tab 砸脸」的根因被 051 / 058 / 060 / 063 declutter 同时解决 — cuts 落地后 panel 不再令人却步，专用 onboarding overlay 价值进一步下降。

与 `feedback_simplify_cut_dont_add`（cut > add）+ `feedback_pet_core_5_no_meta_no_gamification`（5 核心不含 onboarding wizard）冲突。

若后续真出现「新用户 30 秒内不知道怎么用 pet」证据 → 可重启，思路改为「让首次 proactive turn 主动说一句更友好的介绍」而非建独立 overlay。
