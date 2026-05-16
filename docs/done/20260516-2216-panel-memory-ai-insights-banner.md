# PanelMemory ai_insights 类目顶加 "🧠 由宠物自己写" 说明 banner

## 背景

ai_insights cat 含宠物自身反思 / persona / mood / daily_plan 等条目 —— 主要由 LLM proactive cycle / consolidate 自动维护。首次 owner 看到 list 时可能：
- 误以为是手动 todo
- 试图编辑 / 删除 persona_summary 等 protected 条目
- 不知道这段 "代谢"逻辑（宠物日积月累）

CATEGORY_PLACEHOLDERS 已有解释串，但只在新建 / 编辑 modal textarea placeholder 里出现 —— list 视图无视觉提示。

加一段常驻 banner 直接显在 ai_insights cat header 下，首次 hover / scroll 时即可发现。

## 改动

### `src/components/panel/PanelMemory.tsx`

ai_insights cat section 内、items list 之前插入 banner：

```tsx
{catKey === "ai_insights" && (
  <div
    style={{
      background: "var(--pet-tint-purple-bg, var(--pet-color-bg))",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 6,
      padding: "6px 10px",
      marginBottom: 8,
      fontSize: 11,
      color: "var(--pet-color-muted)",
      lineHeight: 1.5,
    }}
  >
    🧠 <strong>这里是宠物自己写的</strong>：proactive cycle / consolidate
    自动维护 <code>persona_summary</code> / <code>current_mood</code> /
    <code>daily_plan</code> / <code>daily_review_&lt;date&gt;</code> 等。
    手动编辑可以，但通常让宠物自己慢慢沉淀更自然。删除一条 = 让宠物"忘记"
    这段反思。
  </div>
)}
```

## 关键设计

- **purple tint bg + 11px muted**：与既有 butler_tasks 段顶黄底 butlerDaily banner 视觉对偶但色调区分（一段是宠物 author / 另一段是 daily summary 数据）。fallback `var(--pet-color-bg)` 防 purple-tint 未定义。
- **空 cat 也显**：onboarding 价值最大的时机 —— owner 还没看到 item 时就该知道"这段不是手动 todo"。
- **列举 4 个 protected items**：persona_summary / current_mood / daily_plan / daily_review_<date> 让 owner 知道"哪些不该手动改"。
- **"删除一条 = 让宠物'忘记'"**：让 owner 知道删除的副作用而不是不可逆错误。
- **不锁定编辑**：banner 文案明说"手动编辑可以" —— 不是 protection，是 informed consent。

## 不做

- **不真正 protect persona_summary 等 items 防误删**：编辑 / 删除是 owner 权利。banner 提醒就好。
- **不加 dismissable / hide forever 按钮**：banner 占位很小（11px 字 + 6px padding）；老用户 hide 心智成本 > 受益。
- **不写测试**：纯 UI 文本添加；视觉验证（开 PanelMemory → 看 ai_insights cat 顶应见 🧠 banner）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.21s
- 改动 ~25 行（banner div + 注释）。既有 cat 渲染 / placeholder / butlerDaily / butlerHistory / item list 路径完全不动。

## TODO 状态

剩 2 条留池：
- PanelTasks "新建任务" + ⇧Enter 创建并立即打开 detail 编辑器
- 桌面 pet 右键加「⏰ 设倒计时 N 分钟 nudge」

## 后续

- 同款 banner 给 user_profile cat（"📝 这里记录用户习惯 / 偏好 / 工作 setup"），让 owner 知道这段是用户自身的稳定档案。
- protected items（persona_summary / current_mood / daily_plan）edit modal 时 input 上方加 amber warning chip "这是宠物自己维护的，编辑可能被下次 consolidate 覆盖"。
- daily_review_<date> 太多时 banner 内加 "📦 N 条 daily review (consolidate 自动归档过期)" 计数链接。
