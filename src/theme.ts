/**
 * 设计令牌：抽出框架级 6 个核心颜色 + 6 对 section tint，通过 CSS 变量挂在
 * `document.documentElement` 上。组件用 `var(--pet-color-bg)` 等引用。
 *
 * 051-part1：GOAL.md「不做深色 / 浅色主题」整改 —— 删除 dark 一档，TOKENS /
 * TINTS / ACCENT_VALUES 全部退化为单值。保留 accent 5 色（这是用户偏好的
 * 主品牌色定制，非主题切换）。
 *
 * 用 CSS var 而非 React Context：
 * - 零运行时 overhead，不挂 Provider 也不串 props
 * - accent 切换不触发任何 React re-render（DOM 属性变化即生效）
 */

/**
 * Accent 调色板：5 选 1。default 沿用既有 sky 蓝；其余 4 色给愿意自定义的
 * advanced 用户。
 */
export type Accent = "default" | "green" | "purple" | "orange" | "rose";

const ACCENT_VALUES: Record<Accent, string> = {
  default: "#0ea5e9", // sky-500
  green: "#10b981", // emerald-500
  purple: "#8b5cf6", // violet-500
  orange: "#f97316", // orange-500
  rose: "#f43f5e", // rose-500
};

/** UI 选项元数据：label / 颜色样本。 */
export const ACCENT_OPTIONS: Array<{ key: Accent; label: string; swatch: string }> = [
  { key: "default", label: "蓝", swatch: ACCENT_VALUES.default },
  { key: "green", label: "绿", swatch: ACCENT_VALUES.green },
  { key: "purple", label: "紫", swatch: ACCENT_VALUES.purple },
  { key: "orange", label: "橙", swatch: ACCENT_VALUES.orange },
  { key: "rose", label: "玫红", swatch: ACCENT_VALUES.rose },
];

export interface ThemeTokens {
  /** 页面 / panel 容器底色 */
  bg: string;
  /** 卡片 / formCard 内层底色 */
  card: string;
  /** 主要文本 */
  fg: string;
  /** 次要文本（hint / placeholder / count） */
  muted: string;
  /** 边框 / 分隔线 */
  border: string;
  /** 主品牌色（active tab / primary button）。 */
  accent: string;
}

/** 6 对 section tint（每对 bg + fg）。 */
export interface ThemeTints {
  /** 紫 — recentSpeeches */
  purpleBg: string;
  purpleFg: string;
  /** 淡紫 — prompt-hints */
  lavenderBg: string;
  lavenderFg: string;
  /** 黄 — tool history / butler 每日小结 */
  yellowBg: string;
  yellowFg: string;
  /** 绿 — feedback */
  greenBg: string;
  greenFg: string;
  /** 橙 — reminders */
  orangeBg: string;
  orangeFg: string;
  /** 蓝 — butler 最近执行 */
  blueBg: string;
  blueFg: string;
  /** 红 — 高紧迫信号（任务过期、危险确认按钮）。 */
  redBg: string;
  redFg: string;
}

export const TOKENS: ThemeTokens = {
  bg: "#f8fafc",
  card: "#ffffff",
  fg: "#1e293b",
  muted: "#64748b",
  border: "#e2e8f0",
  accent: "#0ea5e9",
};

export const TINTS: ThemeTints = {
  purpleBg: "#fdf4ff",
  purpleFg: "#86198f",
  lavenderBg: "#faf5ff",
  lavenderFg: "#6b21a8",
  yellowBg: "#fefce8",
  yellowFg: "#854d0e",
  greenBg: "#f0fdf4",
  greenFg: "#065f46",
  orangeBg: "#fff7ed",
  orangeFg: "#9a3412",
  blueBg: "#f0f9ff",
  blueFg: "#0369a1",
  redBg: "#fef2f2",
  redFg: "#b91c1c",
};

const CSS_VAR_PREFIX = "--pet-color-";
const TINT_VAR_PREFIX = "--pet-tint-";

/** 把驼峰键 `purpleBg` 转 CSS 变量后缀 `purple-bg`。 */
function camelToKebab(s: string): string {
  return s.replace(/([A-Z])/g, "-$1").toLowerCase();
}

/** 把 token + tint 集合写到 `document.documentElement` 的 CSS 变量上。SSR
 *  安全：window 不存在时直接 return。
 *
 *  `accent` 参数：传则覆盖 token 里的 accent 字段，缺省 / "default" 走原
 *  `TOKENS.accent`（sky 蓝）。 */
export function applyTheme(accent?: Accent): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  for (const [key, value] of Object.entries(TOKENS)) {
    let effective = value;
    if (key === "accent" && accent && accent !== "default") {
      effective = ACCENT_VALUES[accent];
    }
    root.style.setProperty(`${CSS_VAR_PREFIX}${key}`, effective);
  }
  for (const [key, value] of Object.entries(TINTS)) {
    root.style.setProperty(`${TINT_VAR_PREFIX}${camelToKebab(key)}`, value);
  }
  if (accent) root.setAttribute("data-accent", accent);
}

const ACCENT_STORAGE_KEY = "pet-accent";

/** 读 accent 偏好；解析失败 / 没存过 / 不在白名单 → "default"。 */
export function getStoredAccent(): Accent {
  if (typeof window === "undefined") return "default";
  try {
    const raw = window.localStorage.getItem(ACCENT_STORAGE_KEY);
    if (raw && (raw === "default" || raw === "green" || raw === "purple" || raw === "orange" || raw === "rose")) {
      return raw;
    }
  } catch {
    // localStorage 禁用 / 配额满 → 默认 default
  }
  return "default";
}

/** 写 accent 偏好。失败静默吞 —— 丢失只让下次启动回 default。 */
export function setStoredAccent(accent: Accent): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(ACCENT_STORAGE_KEY, accent);
  } catch (e) {
    console.error("setStoredAccent failed:", e);
  }
}
