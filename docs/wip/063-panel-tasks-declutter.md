# 063 · PanelTasks 整改 — 7+ audit chip 累积

PanelTasks 19860 行，多 audit chip 不服务 5 核心，违反 064 cut 红线。

- ✅ part1–10：删 17 chip 家族 + 关联 state / useMemo / fetch / 滤链。最新
  part 删 📂 detail.md 字数 hover chip + orphan comment 块（字数 audit）。
  19860 → 18581 行（-1279，约 6.4%）。每 part tsc + vite clean。
- ⏳ part11：⏰ reminderMin hover（交互入口）+ hover action ≤4 + toolbar 极简。需 dev 视觉。
