# Persona 当日 mood entries 导出 CSV（Iter R124）

> 对应需求（来自 docs/TODO.md）：
> Persona 当日 mood entries 导出 CSV：mood_history 当日详情按 `timestamp,motion,text` 三列拼成 CSV 一键复制到剪贴板，便于用户做离线分析（与 R98 memory MD 导出 / R106 chat MD 导出形成"多类数据导出"）。

## 目标

PanelPersona 当日详情已有"复制为 MD"按钮（line 1038-）输出 markdown
段。MD 适合贴笔记复盘，但不适合数据分析（excel / pandas / sql）。

加 "复制为 CSV" 按钮在 MD 旁，输出 `timestamp,motion,text` 三列 CSV：
- timestamp 全 RFC3339（含日期 + 时间，方便跨工具排序 / 过滤）
- motion 接口枚举值（Tap / Flick / Flick3 / Idle）
- text 用户原文（CSV 转义保护逗号 / 换行 / 引号）

## 非目标

- 不写文件 —— 与 R98 / R106 一致，剪贴板覆盖 95% 场景
- 不带 BOM / Excel-friendly UTF-8 magic —— 现代 Excel / Numbers / Google
  Sheets 都能识别 plain UTF-8；BOM 在 vim / less / pandas 反而是 noise
- 不导出当前 motion filter 后的子集 —— 全量当日数据更通用，过滤交给用户
  在表格工具内做

## 设计

### CSV 转义

```ts
function csvEscape(s: string): string {
  if (/[",\n\r]/.test(s)) {
    return `"${s.replace(/"/g, '""')}"`;
  }
  return s;
}
```

含逗号 / 引号 / 换行的字段用双引号包住；内嵌引号 `"` 转 `""`（RFC 4180）。
其它字段原样输出。

### Helper

```ts
function formatDayEntriesAsCsv(entries: MoodEntry[]): string {
  const lines = ["timestamp,motion,text"];
  for (const e of entries) {
    lines.push(
      [csvEscape(e.timestamp), csvEscape(e.motion), csvEscape(e.text)].join(","),
    );
  }
  return lines.join("\n");
}
```

放在文件末（与 `formatDayEntriesAsMarkdown` 同位置）。空 entries 仍输出
header（仅 `timestamp,motion,text`），让用户拿到合法 CSV 而非空字符串。

### state

```ts
const [copiedDayCsv, setCopiedDayCsv] = useState(false);
```

与 `copiedDayMd` 同语义；2s（与 MD 1.5s 略长）反馈窗口够用户切窗口贴。

### 渲染

在 MD 按钮之后插 CSV 按钮（同款 inline style）：

```diff
 {dayEntries.length > 0 && (
   <button ... 复制为 MD ...>
 )}
+{dayEntries.length > 0 && (
+  <button
+    type="button"
+    onClick={async () => {
+      try {
+        await navigator.clipboard.writeText(formatDayEntriesAsCsv(dayEntries));
+        setCopiedDayCsv(true);
+        window.setTimeout(() => setCopiedDayCsv(false), 1500);
+      } catch (e) {
+        console.error("clipboard write failed:", e);
+      }
+    }}
+    title={
+      copiedDayCsv
+        ? "已复制 CSV"
+        : "复制为 CSV：timestamp,motion,text 三列；含逗号 / 换行的字段自动加引号转义。粘贴到 Excel / pandas / sqlite 直接可读。"
+    }
+    style={{...同 MD 按钮配色...}}
+  >
+    {copiedDayCsv ? "已复制" : "复制为 CSV"}
+  </button>
+)}
```

紧贴 MD 按钮，让"两个导出"成对出现；视觉权重一致（同 size / 同 border /
同 hover）。

### 测试

无单测；手测：
- 选有 entries 的当日 → 点 复制为 CSV → toast"已复制" 1.5s
- 粘贴到 Excel：三列识别正常
- 粘贴到 vim / less：可见 header + N 行
- 含逗号的 text → CSV 字段自动加引号
- 含换行的 text → 同上
- 含双引号的 text → 引号被翻 `""`

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | csvEscape + formatDayEntriesAsCsv helper |
| **M2** | copiedDayCsv state + button render |
| **M3** | tsc + build |

## 复用清单

- 既有 R98 / R106 / MD 导出剪贴板 + toast 模式
- 既有 dayEntries / selectedDate state

## 进度日志

- 2026-05-10 05:00 — 创建本文档；准备 M1。
- 2026-05-10 05:08 — M1 完成。`csvEscape` + `formatDayEntriesAsCsv` helper 加在 `formatDayEntriesAsMarkdown` 之下（同模块位置）；RFC 4180 转义（含 `,` / `"` / 换行的字段加引号、内嵌 `"` 翻 `""`）；空 entries 仍输出 header 让用户拿到合法 CSV。
- 2026-05-10 05:11 — M2 完成。`copiedDayCsv` state 加在 `copiedDayMd` 旁；selectedDate 变化时 effect 复位 csv 状态；CSV 按钮插在 MD 按钮之后（同款 inline style + accent color），title 解释适用场景（Excel / pandas / sqlite）。
- 2026-05-10 05:14 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 1.07s)。归档至 done。
