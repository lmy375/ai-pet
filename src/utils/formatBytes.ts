/// B / KB / MB / GB 自适应单位转换。负值 / NaN / Infinity → "0 B" 兜底。
/// KB / MB / GB 都用 1 位小数。> 1 PB 时仍按 GB 显示（个人桌面 app 极不可能
/// 触及）。
///
/// 调用方：PanelMemory 概览的 💾 disk usage chip、PanelSettings「本地数据目录」
/// 的 pet.db size chip 等。
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "0 B";
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(1)} GB`;
}
