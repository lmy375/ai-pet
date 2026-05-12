/**
 * 字号 / 行高 / 字重尺度 token。把散布在各 panel 里 `fontSize: 10/11/12/13/14`
 * 字面量收敛成命名 step，让未来调整字号节奏（如把基准上抬一档）只改一处。
 *
 * 不强制迁移：旧代码继续用数字字面量；新代码 / 重要 surface 用本表的命名。
 * 待迁移完成后再考虑废弃数字字面量。
 *
 * 设计依据：当前主流 fontSize 分布（grep 统计 / 高频在前）
 *   145 × 11    105 × 12    90 × 10    111 × 13(含 13.5)    19 × 14
 *      4 × 16    3 × 13.5    多 ≤ 2 个的 outlier（18 / 20 / 22 / 28 / 32 / 44）
 *
 * 命名风格借鉴 Tailwind 的 `xs/sm/base/md/lg/xl`，便于团队心智模型迁移。
 */

export const text = {
  /** 超小：count / hint / chip badge / placeholder 备注 */
  xs: 10,
  /** 小：muted meta / 次级文字 / chip 多色 */
  sm: 11,
  /** 基础：正文 / 输入 / 表单 label / 按钮 */
  base: 12,
  /** 中：section title 主标 / 卡片正文 / message bubble */
  md: 13,
  /** 中加：略大一档，h3 / panel-level 大标题 */
  lg: 14,
  /** 大：modal h2 */
  xl: 16,
  /** 显示：dashboard stat 巨字 */
  hd: 28,
} as const;

export type TextSize = keyof typeof text;

/**
 * 行高尺度。多数文字走 1.5；稠密代码 / chip 走 1.2-1.3；舒展长段 1.6+。
 */
export const lineHeight = {
  tight: 1.2,
  snug: 1.4,
  base: 1.55,
  relaxed: 1.65,
} as const;

/**
 * 字重尺度。仅常用三档；不引入 100 / 800 / 900 等极值。
 */
export const fontWeight = {
  normal: 400,
  medium: 500,
  semibold: 600,
} as const;

/**
 * 字距尺度。中文场景里 0.1-0.3 是经验区间；title 用 letterSpacing 0.2 让"分块"
 * 视觉感更强；正文 0 / `normal`。
 */
export const letterSpacing = {
  normal: 0,
  wide: 0.1,
  wider: 0.2,
  widest: 0.3,
} as const;
