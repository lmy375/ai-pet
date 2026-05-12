/**
 * 设计令牌系统（迭代 1）：抽出框架级 6 个核心颜色，分 light / dark 两套，
 * 通过 CSS 变量挂在 `document.documentElement` 上。组件用 `var(--pet-color-bg)`
 * 等引用，主题切换时无需重新渲染 React 树 —— CSS var 自动 propagate。
 *
 * 迭代 7 加 6 对 section tint 变量（紫 / 淡紫 / 黄 / 绿 / 橙 / 蓝），用 prefix
 * `--pet-tint-*-{bg,fg}`。light 值精确匹配旧 hardcoded hex，dark 值用低饱和
 * 深色 + high-lightness 反相文字，让"section 类型"色块在 dark 下不刺眼但仍
 * 可读。
 *
 * 用 CSS var 而非 React Context：
 * - 零运行时 overhead，不挂 Provider 也不串 props
 * - 主题切换不触发任何 React re-render（只是 DOM 属性变化）
 * - 渐进迁移友好：组件按节奏改 inline color → var()，老 inline 色继续
 *   按既有方式渲染，互不干扰
 */

export type Theme = "light" | "dark";

/**
 * Accent 调色板：5 选 1。default 沿用既有 sky 蓝；其余 4 色给愿意自定义的
 * advanced 用户。每个 accent 都有 light / dark 一对值，dark 下整体提亮一档
 * 保对比度（与既有 sky 的 0ea5e9 → 38bdf8 升一档同模式）。
 */
export type Accent = "default" | "green" | "purple" | "orange" | "rose";

const ACCENT_VALUES: Record<Accent, Record<Theme, string>> = {
  // sky-500 / sky-400 —— 当前默认主品牌色，保持兼容
  default: { light: "#0ea5e9", dark: "#38bdf8" },
  // emerald-500 / emerald-400
  green: { light: "#10b981", dark: "#34d399" },
  // violet-500 / violet-400
  purple: { light: "#8b5cf6", dark: "#a78bfa" },
  // orange-500 / orange-400
  orange: { light: "#f97316", dark: "#fb923c" },
  // rose-500 / rose-400 —— 偏粉调，与红 tint redFg 有区分（redFg 暗红警示色）
  rose: { light: "#f43f5e", dark: "#fb7185" },
};

/** UI 选项元数据：label / 颜色样本（中性 light hex 当 swatch 即可，dark
 *  下也通过 CSS var 自动跟随）。 */
export const ACCENT_OPTIONS: Array<{ key: Accent; label: string; swatch: string }> = [
  { key: "default", label: "蓝", swatch: ACCENT_VALUES.default.light },
  { key: "green", label: "绿", swatch: ACCENT_VALUES.green.light },
  { key: "purple", label: "紫", swatch: ACCENT_VALUES.purple.light },
  { key: "orange", label: "橙", swatch: ACCENT_VALUES.orange.light },
  { key: "rose", label: "玫红", swatch: ACCENT_VALUES.rose.light },
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
  /** 主品牌色（active tab / primary button）。跨主题语义一致，dark 下提
   *  亮一档保对比度 */
  accent: string;
}

/**
 * Iter 7: 6 对 section tint 变量（每对 bg + fg）。
 *
 * dark bg 走"slate 主底偏色微调"思路：保留色相、把 lightness 拉到 ~10%，
 * 与主背景区分但不抢戏；dark fg 用对应色族的 100/200 阶（high lightness）
 * 保对比度。
 */
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
  /** 红 — 高紧迫信号（任务过期、危险确认按钮）。orange 表示警告但不至于
   *  fail；red 留给"已经出问题 / 立刻看"。 */
  redBg: string;
  redFg: string;
}

export const TOKENS: Record<Theme, ThemeTokens> = {
  light: {
    bg: "#f8fafc",
    card: "#ffffff",
    fg: "#1e293b",
    muted: "#64748b",
    border: "#e2e8f0",
    accent: "#0ea5e9",
  },
  dark: {
    bg: "#0f172a",
    card: "#1e293b",
    fg: "#f1f5f9",
    muted: "#94a3b8",
    border: "#334155",
    accent: "#38bdf8",
  },
};

export const TINTS: Record<Theme, ThemeTints> = {
  light: {
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
  },
  dark: {
    purpleBg: "#251a32",
    purpleFg: "#e879f9",
    lavenderBg: "#221d33",
    lavenderFg: "#d8b4fe",
    yellowBg: "#2a2410",
    yellowFg: "#fde68a",
    greenBg: "#0c2419",
    greenFg: "#86efac",
    orangeBg: "#2b1f10",
    orangeFg: "#fdba74",
    blueBg: "#0c2236",
    blueFg: "#7dd3fc",
    redBg: "#2a1010",
    redFg: "#fca5a5",
  },
};

const CSS_VAR_PREFIX = "--pet-color-";
const TINT_VAR_PREFIX = "--pet-tint-";

/** 把驼峰键 `purpleBg` 转 CSS 变量后缀 `purple-bg`。 */
function camelToKebab(s: string): string {
  return s.replace(/([A-Z])/g, "-$1").toLowerCase();
}

/** 把 `theme` 对应的 token + tint 集合写到 `document.documentElement` 的
 *  CSS 变量上。SSR 安全：window 不存在时直接 return。
 *
 *  `accent` 参数：传则覆盖 token 里的 accent 字段，让用户自选主品牌色生效；
 *  缺省 / "default" 走原 token.accent（sky 蓝）。 */
export function applyTheme(theme: Theme, accent?: Accent): void {
  if (typeof document === "undefined") return;
  const tokens = TOKENS[theme];
  const tints = TINTS[theme];
  const root = document.documentElement;
  for (const [key, value] of Object.entries(tokens)) {
    let effective = value;
    if (key === "accent" && accent && accent !== "default") {
      effective = ACCENT_VALUES[accent][theme];
    }
    root.style.setProperty(`${CSS_VAR_PREFIX}${key}`, effective);
  }
  for (const [key, value] of Object.entries(tints)) {
    root.style.setProperty(`${TINT_VAR_PREFIX}${camelToKebab(key)}`, value);
  }
  // 也把 theme 名字塞到 data-attribute 上，便于将来用 [data-theme="dark"]
  // 选择器写更复杂的覆盖（如 hover / scrollbar 风格）。
  root.setAttribute("data-theme", theme);
  if (accent) root.setAttribute("data-accent", accent);
}

const STORAGE_KEY = "pet-theme";

/** 读 localStorage 偏好；解析失败 / 没存过 → "light" 兜底。 */
export function getStoredTheme(): Theme {
  if (typeof window === "undefined") return "light";
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw === "dark" || raw === "light") return raw;
  } catch {
    // localStorage 禁用 / 配额满 → 默认 light
  }
  return "light";
}

/** 写 localStorage 偏好。失败静默吞 —— 主题偏好丢失只让下次启动回 light，
 *  不该把异常上抛影响主流程。 */
export function setStoredTheme(theme: Theme): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch (e) {
    console.error("setStoredTheme failed:", e);
  }
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

/** 写 accent 偏好。失败静默吞 —— 同 theme，丢失只让下次启动回 default。 */
export function setStoredAccent(accent: Accent): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(ACCENT_STORAGE_KEY, accent);
  } catch (e) {
    console.error("setStoredAccent failed:", e);
  }
}
