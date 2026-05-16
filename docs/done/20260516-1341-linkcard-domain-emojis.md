# detail.md LinkCard 特殊域名 emoji

## 背景

TODO 上 auto-proposed 一条："detail.md 内嵌 https 链接：渲染 LinkCard 时显特殊域名 emoji（GitHub 🐙 / Linear 📐 / Figma 🎨 / Notion 📓 / YouTube ▶️），其它仍 📎，让常用引用一眼可见。"

LinkCard 已经把 bare URL 渲成「📎 hostname」chip，但所有域名一个 emoji 失去信息密度。owner 看到一排 📎 chip 还得读 hostname 才知道是 GitHub PR / Linear ticket / Figma 设计稿。常用引用源加专属 emoji 让"这是什么类型的链接"一眼可读。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 新增 `DOMAIN_EMOJI_MAP`

```ts
const DOMAIN_EMOJI_MAP: Record<string, string> = {
  "github.com": "🐙",
  "gitlab.com": "🦊",
  "linear.app": "📐",
  "figma.com": "🎨",
  "notion.so": "📓",
  "notion.site": "📓",
  "youtube.com": "▶️",
  "youtu.be": "▶️",
  "docs.google.com": "📄",
  "drive.google.com": "🗂️",
  "twitter.com": "🐦",
  "x.com": "🐦",
  "stackoverflow.com": "📚",
  "npmjs.com": "📦",
  "news.ycombinator.com": "🟧",
  "reddit.com": "👽",
  "arxiv.org": "📜",
  "wikipedia.org": "🌐",
  "medium.com": "✍️",
};
```

19 个常用引用源 emoji 覆盖。未命中 → 退化 📎 通用 emoji（原有行为不变）。

#### 新 `pickLinkEmojiAndLabel(url)` pure helper

```ts
function pickLinkEmojiAndLabel(url: string): { emoji: string; label: string } {
  let host: string;
  try {
    host = new URL(url).hostname.toLowerCase();
  } catch {
    return { emoji: "📎", label: url };
  }
  const cleaned = host.startsWith("www.") ? host.slice(4) : host;
  // 完全相等优先
  const direct = DOMAIN_EMOJI_MAP[cleaned];
  if (direct) return { emoji: direct, label: cleaned };
  // 子域名 fallback：'gist.github.com' / 'api.github.com' 命中 'github.com'
  for (const key of Object.keys(DOMAIN_EMOJI_MAP)) {
    if (cleaned.endsWith("." + key)) {
      return { emoji: DOMAIN_EMOJI_MAP[key], label: cleaned };
    }
  }
  return { emoji: "📎", label: cleaned };
}
```

#### `LinkCard` 用新 emoji

```tsx
function LinkCard({ url }) {
  const { emoji, label } = pickLinkEmojiAndLabel(url);
  return <a ...>{emoji} {label}</a>;
}
```

原 `let label = url; try { label = new URL(url).hostname; } catch {}` 由新 helper 一并替代。

## 关键设计

- **完全相等优先 + 子域名 fallback 双语义**：`github.com` 命中 🐙，`gist.github.com` 也命中 🐙（`endsWith(".github.com")`）。让常见子域名（gist / api / raw / wiki）共享父域 emoji。逻辑顺序保证 `notion.site` 不被 `notion.so` 的 `.notion.so` 误命中（完全相等先查）。
- **`www.` 前缀剥**：`www.github.com` 与 `github.com` 视作同一站点 —— 浏览器层早归一化，emoji 也该一致。仅 leading `www.` 剥（不剥 `app.` / `m.` 等真子域）。
- **`pickLinkEmojiAndLabel` pure helper 而非 LinkCard 内联**：助拆解 + 未来若 ChatPanel / mini chat 也想要 emoji 链接，复用直接。pure 函数也便于将来加单测。
- **未命中 fallback 📎**：保 backward-compat —— 原所有 URL 都显 📎，现 19 个 known domain 升级特殊 emoji，其它仍 📎，行为渐进。
- **emoji 选取依据**：跟随常见社区约定（🐙 GitHub Octocat 致敬 / 🦊 GitLab logo / ▶️ YouTube play / 🟧 HN orange / 👽 Reddit alien），让中文圈 + 英文圈用户都能识别。
- **不加 favicon fetch**：fetch favicon 需要网络请求 + 缓存 + 安全审计（任意 url 触发 IO 是攻击面），且需要 fallback 处理 dead domain。19 个硬编码 emoji 是 zero-runtime-cost 的"最易识别"近似。
- **顺序无关**：DOMAIN_EMOJI_MAP 是 Record；`Object.keys()` 顺序在现代 V8 / JavaScriptCore 中按插入序，但匹配逻辑取首个命中即返回 —— 由于子域名收敛规则（一个 URL 只属于一个 root domain），不会命中多 key。

## 不做

- **不写测试**：纯字符串 split + Record lookup，逻辑 ~30 行；19 个 emoji 是配置常量，单测就是把 mapping 写两遍。视觉验证（写各域名 URL 入 detail.md → 看 chip emoji 是否对）足够。
- **不外露 emoji 配置给用户自定义**：19 个常见域名已覆盖 80%+ 引用源；用户自定义需要 settings UI + persistence，复杂度大。等真有用户诉求再加。
- **不动 inlineMarkdown.UrlLink**：那是 chat 流 / mini chat 用的纯蓝色下划线链接，不涉及 detail.md 工作流。chat 流里链接形态保简洁。
- **不加"链接预览卡 + 标题摘要"**：抓 OG meta 是更富的体验，但要后端 fetch + 缓存 + UI 风险评估，远超本 iter 范围。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.28s
- 改动 ~70 行（DOMAIN_EMOJI_MAP 25 + pickLinkEmojiAndLabel 25 + LinkCard 调用 -8 + 注释）；既有 LinkCard 调用方 / renderDetailTextWithLinkCards / parseDetailMdWithImages 路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 5 条，余 1 条留池：
- 任务行 hover preview 段也走 LinkCard

## 后续

- emoji 命中后给 chip 加 site-specific tint 色（如 GitHub black / Linear purple / Figma red）—— 让 chip 视觉更接近"原站品牌"。当前统一 card bg + 单色 emoji 足够轻量。
- 让 owner 通过 PanelSettings 加自定义 mapping（公司内 wiki / GitLab 私有部署 / Confluence 等）—— 配置文件 schema 设计需考虑。
- 把 DOMAIN_EMOJI_MAP 抽到 module-level shared utils 让 PanelMemory / 未来 chat 也能复用 —— 等真有第 2 个 callsite 再抽。
