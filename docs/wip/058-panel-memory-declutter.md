# 058 · PanelMemory 瘦身

三层 chip 雪崩（主 toolbar 14 / per-cat 子 9 / per-item hover 16），空 cat
占大块垂直，🧠 explainer banner 永久 inline。

- ✅ part1：删 ai_insights cat 常驻 🧠 explainer banner（含 📦 daily_review
  计数 chip）— 都是 audit-style 元数据，不服务 pet 5 核心。tsc + vite build clean。
- ⏳ part2：主 toolbar 14→≤5、per-cat 子 toolbar 整删、per-item hover 16→4、
  空 cat 折叠 ≤40px、stats 整合一行、per-item footer 默认隐。皆需 dev 视觉。
- ⏳ part3：🧠 banner 替换为 cat header 旁 ⓘ icon + popover（评估是否真需）。
