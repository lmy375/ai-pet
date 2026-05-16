/// session 下拉 / 跨会话搜索结果 / PanelMemory items 共享的"月份分组 key"。
/// 返回 4 档：
/// - `_thisMonth` / `_lastMonth`：本月 / 上月（与 `now` 比较）
/// - `YYYY-MM`：更早月份的具体 ISO
/// - `older`：无效 / 空 timestamp 兜底（旧迁移期数据）
///
/// `now` 作参数让 IIFE 一次性算出再传，避免每条 item 重 new Date()，也便于
/// 测试（不依赖 wall clock）。
export function monthKeyFromIso(iso: string, now: Date): string {
  if (iso.length < 7) return "older";
  const yyyymm = iso.slice(0, 7);
  const curYm = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  if (yyyymm === curYm) return "_thisMonth";
  const prev = new Date(now.getFullYear(), now.getMonth() - 1, 1);
  const prevYm = `${prev.getFullYear()}-${String(prev.getMonth() + 1).padStart(2, "0")}`;
  if (yyyymm === prevYm) return "_lastMonth";
  return yyyymm;
}

/// 月份 key → 中文 label。`_pinned` 虚拟 key 给 session 下拉 / PanelMemory
/// items 共用，pinned 段不归月份单列首段；其它 key 与 `monthKeyFromIso` 对偶。
export function monthLabelOf(key: string): string {
  if (key === "_pinned") return "📌 钉住";
  if (key === "_thisMonth") return "本月";
  if (key === "_lastMonth") return "上月";
  if (key === "older") return "更早";
  return key; // "YYYY-MM"
}
