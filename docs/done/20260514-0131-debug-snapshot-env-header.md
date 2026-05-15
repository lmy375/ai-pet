# PanelDebug 快照顶部加"环境信息"段

## 背景

上轮把 `app_version` Tauri 命令暴露给 Settings chip。下一个高价值消费方是 `PanelDebug.buildDebugMarkdownSnapshot` —— 用户复制快照给同事 / 贴 issue 时，**当前版本号 + schema_version** 是排查 70% 问题需要的信息（同样的 bug 在 v0.1.0 早已修了 / schema v4 之前的人没有 task_stats 命令）。

现在 snapshot 没有任何环境信息，只有运行时数据。

## 改动

`src/components/panel/PanelDebug.tsx`：

### 新增 state + 挂载 fetch

```ts
const [envInfo, setEnvInfo] = useState<{
  appVersion: string;
  schemaVersion: number;
} | null>(null);

useEffect(() => {
  (async () => {
    const [v, s] = await Promise.all([
      invoke<string>("app_version").catch(() => ""),
      invoke<{ schema_version: number }>("get_db_stats")
        .then(d => d.schema_version)
        .catch(() => 0),
    ]);
    setEnvInfo({ appVersion: v, schemaVersion: s });
  })();
}, []);
```

### snapshot 顶部插入"环境"段

`buildDebugMarkdownSnapshot` 在 `# Pet 调试快照（ts）` 头之后、`陪伴 N 天` 之前，插一段：

```
## 环境
- app: pet v{appVersion}
- schema: v{schemaVersion}
- 平台: {navigator.platform}
- 时间: {ts}
```

`navigator.platform` 多数 webview 仍正常返回（Mac、Win、Linux），用作粗粒度 OS 分类。失败 / 空 → 跳过该行。

envInfo === null 时该 section 整体不挂（旧 backend 兼容；快照仍能用，只是少环境段）。

### useCallback deps 更新

`buildDebugMarkdownSnapshot` 的依赖数组加 `envInfo`，保证 fetch 后下次复制走带环境信息的版本。

## 不做

- 不暴露 build_date / commit hash：build.rs 嵌入需要额外脚手架；版本号 + schema 已够日常 triage
- 不嵌 user_agent 全文：navigator.platform 简短够用，user_agent 太长污染 markdown
- 不让 snapshot 自动包含 OS arch / cores：与 bug 关联弱

## 验收

- `npx tsc --noEmit` ✅
- PanelDebug 切去任意 tab，回来后 mount 触发 fetch
- 点"复制快照" → markdown 顶部含 `## 环境` + app/schema/平台 三行
- "issue 模板复制"同样含该段（共用 buildDebugMarkdownSnapshot）

## 完成

- [x] PanelDebug.tsx: envInfo state + mount effect
- [x] buildDebugMarkdownSnapshot 嵌环境段 + deps
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
