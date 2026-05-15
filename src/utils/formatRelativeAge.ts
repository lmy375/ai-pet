/// 相对时间分桶：把 age 毫秒转成"X 分钟前 / X 小时前 / X 天前"。
///
/// 不处理 < 60s 的边界（"刚刚 / 刚创建 / 不到 1 分钟"等），由调用方按业务
/// 语义补 ——不同 panel 选词偏好不同（更新 vs 创建 vs 主动开口），抽不到一处。
/// 也不处理 ageMs < 0（caller 应保证 now >= ts）。
///
/// 调用方：PanelMemory.formatLastUpdated、PanelTasks.formatRelativeAge、
/// PanelChat 的内联 itemMeta 计算等。
export function formatRelativeAgeBuckets(ageMs: number): string {
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)} 分钟前`;
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)} 小时前`;
  return `${Math.floor(ageMs / 86_400_000)} 天前`;
}
