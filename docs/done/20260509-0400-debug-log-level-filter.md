# PanelDebug 日志区按 level 过滤（Iter R99）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 日志区按 level 过滤：log 输出区现显全部行；加 chip 行 "全部 / ERROR / WARN / INFO" 多选过滤（与决策日志多选 chip 同款），让 debug 时聚焦 ERROR / WARN 不被 INFO 噪音淹没。

## 目标

PanelDebug 底部日志区现在 dump 全部 ring buffer 行（每行已按 ERROR 红 /
WARN 黄 / INFO 灰着色），但典型 debug 场景里 INFO 行会很多（heartbeat /
poll 心跳等），把 ERROR / WARN 淹没。每次都得肉眼扫高亮才能定位问题。

加 level 多选 chip：默认显全部；用户可勾选只看 ERROR + WARN，临时屏蔽
INFO 噪音。与决策日志的 R83 多选 chip 模式同款（`Set<level>` empty = "全部"）。

## 非目标

- 不持久化 —— 临时 debug 视角，与决策日志 / 工具风险等过滤同语义
- 不引入"按内容关键字搜索"—— 那是另一个交互维度，等后续如有需求再加
- 不改 backend ring buffer 行序列 / 容量

## 设计

### Hoist `chipStyle` 到 module 级

现 `chipStyle` 在决策日志 section 的 IIFE 里 closure 定义。本轮加第二个
chip 行复用它 → 抽到 module-top 命名为 `multiSelectChipStyle`，让后续
chip 列复用：

```ts
/// 通用多选 chip 样式：active 走 accent 填充 + 白字；inactive 走 accent
/// 40% alpha 边框 + card 底 + fg 字（与决策日志 R84 同款，让"非 kind 维度
/// 的过滤 chip"视觉统一）。
const multiSelectChipStyle = (
  isActive: boolean,
  accent: string,
): React.CSSProperties => ({
  padding: "2px 8px",
  fontSize: "10px",
  borderRadius: "10px",
  border: `1px solid ${isActive ? accent : `${accent}66`}`,
  background: isActive ? accent : "var(--pet-color-card)",
  color: isActive ? "#fff" : "var(--pet-color-fg)",
  cursor: "pointer",
  fontWeight: 600,
  fontFamily: "inherit",
});
```

替换决策日志现有 `chipStyle` 调用：使用名称 `multiSelectChipStyle`，行为
一致。

### state + filter

```ts
type LogLevel = "ERROR" | "WARN" | "INFO";
const [logLevels, setLogLevels] = useState<Set<LogLevel>>(() => new Set());

const logLevelCounts = useMemo(() => {
  let err = 0, warn = 0, info = 0;
  for (const line of logs) {
    if (line.includes("ERROR")) err++;
    else if (line.includes("WARN")) warn++;
    else info++;
  }
  return { ERROR: err, WARN: warn, INFO: info };
}, [logs]);

const filteredLogs = useMemo(() => {
  if (logLevels.size === 0) return logs;
  return logs.filter((line) => {
    const lvl: LogLevel =
      line.includes("ERROR") ? "ERROR" :
      line.includes("WARN") ? "WARN" : "INFO";
    return logLevels.has(lvl);
  });
}, [logs, logLevels]);
```

`includes("ERROR")` 是简化的检测：rust env_logger 输出格式 `[2026-05-09T...
ERROR pet::xxx]`，"ERROR" 在每行 level 段唯一出现，做 substring 命中无歧
义。WARN 同理。其它 → INFO（含 DEBUG / TRACE 等更低级别）。

### 渲染

在现有 dark log scroll area 之上插入小 chip 行（with light theme bg，反差
浅黑日志区）：

```tsx
<div style={{
  display: "flex",
  alignItems: "center",
  gap: 6,
  padding: "6px 16px",
  borderBottom: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-bg)",
  flexWrap: "wrap",
}}>
  <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>level:</span>
  <button
    type="button"
    onClick={() => setLogLevels(new Set())}
    style={multiSelectChipStyle(logLevels.size === 0, "#475569")}
    title="显示全部级别。点击清空多选过滤。"
  >
    全部 {logs.length}
  </button>
  {(["ERROR", "WARN", "INFO"] as const).map((lvl) => {
    const accent = lvl === "ERROR" ? "#dc2626" : lvl === "WARN" ? "#f59e0b" : "#475569";
    const active = logLevels.has(lvl);
    return (
      <button
        key={lvl}
        type="button"
        onClick={() => {
          setLogLevels((prev) => {
            const next = new Set(prev);
            if (next.has(lvl)) next.delete(lvl);
            else next.add(lvl);
            return next;
          });
        }}
        style={multiSelectChipStyle(active, accent)}
      >
        {lvl} {logLevelCounts[lvl]}
      </button>
    );
  })}
  {logLevels.size > 0 && (
    <span style={{ fontSize: 10, color: "var(--pet-color-muted)", marginLeft: "auto" }}>
      显示 {filteredLogs.length} / {logs.length}
    </span>
  )}
</div>
```

accent 配色与日志体内 ERROR 红 / WARN 黄 / INFO 灰对应，让"chip 颜色 ↔
日志正文颜色"形成视觉锚定。

### 渲染替换

把 `logs.map(...)` 改成 `filteredLogs.map(...)`；空状态分两种：

```tsx
{filteredLogs.length === 0 ? (
  <div style={{ color: "#64748b", textAlign: "center", marginTop: "40px" }}>
    {logs.length === 0 ? "暂无日志。聊天和操作会产生日志。" : "当前 level 过滤无匹配日志"}
  </div>
) : (
  filteredLogs.map(...)
)}
```

### 测试

无单测；手测：
- 默认 set 空 → 显全部 logs
- 勾 ERROR → 只显 ERROR 行
- 勾 ERROR + WARN → 显 ERROR + WARN
- 全选 ERROR / WARN / INFO → 等价默认（虽 set 非空但全部命中），可正常显
- 切到 dark 主题 → chip 行 bg / 文字跟切；日志区保持 dark terminal 不变

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | hoist chipStyle 到 multiSelectChipStyle module-level |
| **M2** | state + memo + chip 行 + 替换 render |
| **M3** | tsc + build |

## 复用清单

- 既有 R83 多选 Set 模式
- 既有 R84 accent 40% alpha 边框风格
- 既有 logs state / clear_logs 不动

## 进度日志

- 2026-05-09 04:00 — 创建本文档；准备 M1。
- 2026-05-09 04:08 — M1 完成。`multiSelectChipStyle` 抽到模块顶 const fn；决策日志 IIFE 内 chipStyle 改为 `const chipStyle = multiSelectChipStyle;` 别名（保持调用点改动最小）。
- 2026-05-09 04:14 — M2 完成。`LogLevel` type + `logLevels: Set<LogLevel>` state（默认 empty=全部）；`logLevelCounts` / `filteredLogs` useMemo；dark log scroll area 之前插 chip 行（accent: ERROR 红 / WARN 黄 / INFO 灰）；render 改用 filteredLogs；空状态分两种（`logs.length === 0` 占位 vs 过滤无命中）；显示行末 muted "显示 N / M" 仅在过滤激活时出现。
- 2026-05-09 04:18 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 937ms)。归档至 done。
