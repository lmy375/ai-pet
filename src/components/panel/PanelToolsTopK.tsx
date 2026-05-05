/**
 * R97: 工具调用频次 top-K 卡片。从 PanelDebug 既有 toolCallHistory ring
 * buffer 派生"最近 N 次调用里 top 5 工具"。和 PanelStatsCard / PanelToneStrip
 * 同款"派生统计"独立小卡片定位，让"宠物最依赖什么"一行可见。
 *
 * Props 解耦：只取 `name` 字段而非完整 ToolCallRecord，未来后端 schema
 * 调整只要保留 name 不影响这里。
 */
interface Props {
  history: { name: string }[];
}

export function PanelToolsTopK({ history }: Props) {
  if (history.length === 0) return null;
  const counts = new Map<string, number>();
  for (const r of history) {
    counts.set(r.name, (counts.get(r.name) ?? 0) + 1);
  }
  const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
  if (top.length === 0) return null;
  return (
    <div
      style={{
        padding: "8px 16px",
        borderBottom: "1px solid var(--pet-color-border)",
        background: "var(--pet-color-bg)",
        display: "flex",
        alignItems: "baseline",
        gap: 12,
        flexWrap: "wrap",
        fontSize: 12,
      }}
      title="按工具名汇总当前 ring buffer 中的调用次数；ring buffer 由后端 R4 持久化，cap 见 src-tauri/src/tools/。"
    >
      <span style={{ color: "var(--pet-color-muted)", fontSize: 11 }}>
        🔧 最常用工具（近 {history.length} 次）
      </span>
      {top.map(([name, count], i) => (
        <span
          key={name}
          style={{ display: "inline-flex", alignItems: "baseline", gap: 4 }}
        >
          <span style={{ color: "var(--pet-color-muted)", fontSize: 10 }}>
            #{i + 1}
          </span>
          <span
            style={{
              fontFamily: "'SF Mono', 'Menlo', monospace",
              color: "var(--pet-color-fg)",
              fontWeight: 500,
            }}
          >
            {name}
          </span>
          <span style={{ color: "var(--pet-color-accent)", fontWeight: 600 }}>
            × {count}
          </span>
        </span>
      ))}
    </div>
  );
}
