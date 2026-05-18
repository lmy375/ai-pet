# ChatMini「📤 复制 N 行最近对话」chip — 已实现 pivot drop（iter #577）

## Discovery

TODO 提案：「ChatMini「📤 复制 N 行最近对话」chip：bubble 列表上方
常驻，click 复制最近 N 条 user+assistant 对话纯文本 — 跨工具粘贴」。

**事实：功能已存在多 iter，在 ChatMini.tsx:1997-2096**。

## Existing implementation

```tsx
// ChatMini.tsx:501-502: state
const [copyMenuOpen, setCopyMenuOpen] = useState(false);

// ChatMini.tsx:1997+ 常驻按钮（absolute top-right of chat 区域）
<button onClick={() => setCopyMenuOpen((v) => !v)} title="复制最近 N 条对话到剪贴板（弹菜单选 N）">
  📋
</button>
{copyMenuOpen && (
  <div /* popover */>
    <div>复制最近</div>
    {[3, 5, 10].map((n) => (
      <button onClick={() => copyRecentN(n)}>{n} 条</button>
    ))}
  </div>
)}
```

`copyRecentN(n)` 实现：取最后 N 条 user + assistant message，拼带角
色前缀（如 「user: ...\nassistant: ...」）写到剪贴板。

## TODO 字面 vs 实际差异

| 属性 | TODO | 实际实现 |
|------|------|---------|
| emoji | 📤 | 📋 |
| 位置 | 「bubble 列表上方常驻」| absolute top-right of chat 区域（chat header area，与 ⛶ 最大化按钮同行）|
| 交互 | 单 click 复制 N 条 | click → 弹菜单选 3/5/10 |

功能一致：核心都是「复制最近 N 条 user+assistant 对话纯文本到剪贴板
供跨工具粘贴」。

## Why TODO 提案此功能

可能因为：
- 按钮在角落 + 小 emoji 视觉权重低，新用户没注意
- 弹菜单设计让单 click 看不出「N 条」选项 — 需点开后才知

## Decision

**不实现**。功能已完整：
- 常驻可见（chat header 右上角）
- 多 N 选项（3 / 5 / 10）
- copyRecentN backend 已成熟
- title attr 说明用途

procedure 教训：第 4 次 ChatMini 范畴的 already-implemented pivot（前
3 次：iter #566 bubble ⏱ rel chip / iter #565 PanelMemory updates count
substrate gap / iter #567 /aliases substrate gap）。propose 「ChatMini
{某 chip}」前先 grep `ChatMini.tsx` 内对应 emoji / 关键字 — 这个 file
chip 数量太多，新提案容易碰撞。

可考虑在 ONBOARDING / GOAL.md 加段 reference：「ChatMini chip family
list — 提 chip 前先 audit」。但 self-referential cycle 风险高，暂不
做。

## Future iters (out of scope)

- **改 emoji 为 📤**：与 TODO 字面统一。但 📋 已是 codebase
  conventions for clipboard copy（PanelMemory 单段 .md 复制 / PanelTasks
  📋 复制 detail 等），统一在 📋 更一致 — 不切
- **chip 加默认数字 hint**：「📋 N 5」或类似让首次访客一眼看出「能
  选 N」— 但增加视觉密度。低优先
- **加 N=20 / 50 选项**：当前 3/5/10；长对话 cross-tool 粘贴可能想
  全量复制。但 N 太大 → 剪贴板 token 多 + 粘贴目标可能截断。按需
