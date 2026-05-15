// PanelChat 任务管理 slash 命令（/done /cancel /retry）共享的纯函数。
//
// fetch 由调用方 invoke("task_list") 完成；本模块只对已拿到的 titles 数组做
// 匹配 + 文案渲染，便于复用 + 未来扩到新 slash 命令（如 /detail 之类）。
//
// 不依赖 React / Tauri —— 纯字符串处理。

export type TaskResolution =
  | { kind: "found"; title: string }
  | { kind: "none" }
  | { kind: "multi"; candidates: string[] };

/// 在 titles 上对 query 做 fuzzy 匹配。优先 exact 字面相等；其次走
/// case-insensitive substring。substring 唯一命中→found；0 命中→none；
/// 多命中→multi（caller 据此渲染候选）。query 头尾空白先 trim。
///
/// caller 负责 status 预过滤（例如 /retry 先把非 Error 任务剔除再传入）。
export function matchTaskByQuery(query: string, titles: string[]): TaskResolution {
  const q = query.trim();
  const exact = titles.find((t) => t === q);
  if (exact !== undefined) return { kind: "found", title: exact };
  const qLower = q.toLowerCase();
  const candidates = titles.filter((t) => t.toLowerCase().includes(qLower));
  if (candidates.length === 1) return { kind: "found", title: candidates[0] };
  if (candidates.length === 0) return { kind: "none" };
  return { kind: "multi", candidates };
}

/// 多命中候选列表的反馈文案。最多 5 条 `· title` 一行一条，超出在末尾补
/// `…还有 N 条`。`domainHint` 让 retry 形成"匹配到 N 条 Error 任务"的措辞；
/// done / cancel 传空串即可（"匹配到 N 条任务"）。
export function formatMultiHitMessage(
  query: string,
  candidates: string[],
  domainHint: string,
): string {
  const preview = candidates
    .slice(0, 5)
    .map((t) => `· ${t}`)
    .join("\n");
  const more =
    candidates.length > 5 ? `\n…还有 ${candidates.length - 5} 条` : "";
  return `⚠️ "${query.trim()}" 匹配到 ${candidates.length} 条${domainHint}任务，请输完整或更具体的标题：\n${preview}${more}`;
}
