# PanelDebug 加「⌚ uptime + 版本」复制 chip（iter #488）

## Background

PanelDebug toolbar 已有「📸 抓快照 A」（完整 markdown dump）和「📋
logs 路径」、「📋 cron 配置」等单点信息复制 chip。但 issue / triage
最常用「pet 版本 + schema 版本 + 已运行多久」三段信息 — 完整 snapshot
太重，逐 chip 拼接太碎。

本 iter 加「⌚ uptime + 版本」单击 chip — 拼一行 markdown「pet vX · schema
vY · 已运行 N · 平台」复制。

## Changes

### `src/components/panel/PanelDebug.tsx`

紧贴「📋 logs 路径」之后插：

```tsx
{envInfo && (
  <button
    onClick={async () => {
      const parts: string[] = [];
      if (envInfo.appVersion) parts.push(`pet v${envInfo.appVersion}`);
      if (envInfo.schemaVersion > 0) parts.push(`schema v${envInfo.schemaVersion}`);
      if (envInfo.bootedAtMs !== null) {
        const elapsedSecs = Math.floor((Date.now() - envInfo.bootedAtMs) / 1000);
        const fmt = (secs: number) => {
          if (secs < 60) return `${secs} 秒`;
          if (secs < 3600) return `${Math.floor(secs / 60)} 分`;
          if (secs < 86400) {
            const h = Math.floor(secs / 3600);
            const m = Math.floor((secs % 3600) / 60);
            return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`;
          }
          const d = Math.floor(secs / 86400);
          const h = Math.floor((secs % 86400) / 3600);
          return h > 0 ? `${d} 天 ${h} 小时` : `${d} 天`;
        };
        parts.push(`已运行 ${fmt(elapsedSecs)}`);
      }
      const plat = typeof navigator !== "undefined" ? navigator.platform : "";
      if (plat) parts.push(plat);
      const md = parts.join(" · ");
      try {
        await navigator.clipboard.writeText(md);
        setDebugExportMsg(`⌚ 已复制：${md}`);
      } catch (e) {
        setDebugExportMsg(`复制失败：${e}`);
      }
      window.setTimeout(() => setDebugExportMsg(""), 3500);
    }}
    title={"复制一行 markdown 含 pet 版本 + schema 版本 + 已运行 + 平台 — issue / triage 场景一站式 paste。"}
  >
    ⌚ uptime + 版本
  </button>
)}
```

### 复用既有 envInfo state

- 既有 `appVersion / schemaVersion / bootedAtMs` 都在 envInfo state 内
  （iter #448 在加 uptime field 时建立）
- 复用同 formatUptime 算法（4 段：秒 / 分 / 小时 + 分 / 天 + 小时）保
  与既有「📸 抓快照 A」snapshot 中 uptime 行一致
- `navigator.platform` 提供粗 OS 分类（MacIntel / Win32 / Linux x86_64
  等）

### 输出格式

```
pet v1.2.3 · schema v42 · 已运行 3 小时 15 分 · MacIntel
```

中点分隔；缺失字段（旧 backend 缺 app_version / get_db_stats / 启动
不足 120s 时 bootedAtMs=null）自动跳过 — 不显「 ·  · 」空段。

## Key design decisions

- **chip 而非新建 Tauri command**：所有数据已在 envInfo state 内（mount
  fetch + 派生）— 派生即可，零 backend 改动
- **一行 markdown 而非多行 H2 + bullets**：与 「📋 cron 配置」H2 + bullets
  风格相同 family，但本 chip 是「最少必要 triage 头」轻量入口；多行
  会让 issue / Slack 短消息场景显得啰嗦
- **复用 formatUptime 而非提取 utils**：仅两 callsite（既有 snapshot
  + 本 chip）— 提取 utils 收益不显。如未来第三个 callsite 出现再
  refactor
- **`envInfo &&` gate**：env data 未加载时 chip 隐藏，避免「pet v · schema
  v · 已运行 ?」无意义 paste
- **`setDebugExportMsg` toast 内含具体复制内容**：与「📋 logs 路径」/
  「📋 cron 配置」同 toast pattern — owner 即时验证「我复制的是这个」
  无需粘出来看
- **`navigator.platform` 已 deprecated 但仍 work**：MDN 弃用提示但
  Chrome / Firefox / Safari / Tauri WKWebView 都仍返合理值；本 chip
  是粗 OS 分类辅助 audit 入口，精度需求低
- **不写 unit test**：纯字符串拼接 + clipboard 副作用；逻辑 trivial。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动 — 派生自既有 envInfo
- 手测：PanelDebug toolbar 「📋 logs 路径」之后「⌚ uptime + 版本」chip
  → click → 顶部 toast 显具体复制内容「⌚ 已复制：pet v… · schema v…
  · 已运行 … · MacIntel」→ 粘到 markdown 编辑器看单行展开
