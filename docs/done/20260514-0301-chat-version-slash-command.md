# PanelChat `/version` slash 命令

## 背景

Settings 页面 chip 与 PanelDebug 快照都展示 `pet vX.Y.Z` + `schema vN` + `平台`，但都需要切 tab。在聊天里要快速"看一眼我跑的是哪版"还得绕。

加 `/version` 让聊天就地拿到 3 行系统信息 —— bug 报告 / 跨设备对比时立刻可贴。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 在 `/mood` 之后插 `{ name: "version", description: "查看 pet 版本 / schema / 平台", parametric: false }`
- `SlashAction` 加 `{ kind: "version" }`
- `parseSlashCommand` 加 `case "version": return { kind: "version" }`

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "version"`：

```ts
try {
  const [v, s] = await Promise.all([
    invoke<string>("app_version").catch(() => ""),
    invoke<{ schema_version: number }>("get_db_stats")
      .then(d => d.schema_version)
      .catch(() => 0),
  ]);
  const plat = typeof navigator !== "undefined" ? navigator.platform : "";
  const lines: string[] = [];
  lines.push(v ? `🐾 pet v${v}` : "🐾 pet（版本号缺失）");
  if (s > 0) lines.push(`schema v${s}`);
  if (plat) lines.push(`平台 ${plat}`);
  pushLocalAssistantNote(lines.join("\n"));
} catch (e) {
  pushLocalAssistantNote(`/version 失败：${e}`);
}
```

与 PanelDebug 的"环境"段格式（`app: pet vX.Y.Z` / `schema: vN` / `平台:`）保持文案相似但更紧凑 —— /version 是聊天行内的"印一行"用法，不需要 `app:` `schema:` 前缀让它更长。

## 不做

- 不带 `时间:`：用户聊天里已经知道当下时间，没必要重复
- 不带 build_date / commit hash：上游 commands::app::app_version 当下只暴露 `env!("CARGO_PKG_VERSION")`
- 不复用 PanelDebug 的 `envInfo` state：模块独立 + 命令调用便宜（编译期 env + 单 SQL count），不需要共享缓存

## 验收

- `npx tsc --noEmit` ✅
- 聊天 `/version` → 3 行版本信息（subdued bubble，与其它 slash 反馈视觉一致）
- 后端缺命令 → 仍输出非空 fallback ("pet（版本号缺失）"），不闪空

## 完成

- [x] slashCommands.ts: 注册 + parser
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
