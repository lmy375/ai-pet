import { text, lineHeight, fontWeight, letterSpacing } from "../../text";

/// 跨面板共享 section 标题：accent 小圆点 + halo + 标题 + 可选副标题/右侧
/// 操作槽。统一各 panel 的 sectionTitle 视觉（PanelMemory / PanelTasks 用
/// 13.5px+borderBottom 风；PanelSettings 用 13.5px 无下边线；PanelPersona
/// Section 已经用了圆点 + halo —— 把那套抽出来给所有 panel 共用）。
///
/// 用法：
///   <SectionTitle>外观</SectionTitle>
///   <SectionTitle subtitle="近期主动开口节奏">陪伴时长</SectionTitle>
///   <SectionTitle right={<button>...</button>}>新建任务</SectionTitle>
///
/// `divider`：底部加一条 border-bottom（PanelMemory / PanelTasks 原视觉）。
/// 默认 false —— 大多 section 外层已是 card，再加下边线显沉重。需要明显
/// 分隔（如同 card 内多 sub-section）才打开。
export function SectionTitle({
  children,
  subtitle,
  right,
  divider,
  /// dot：accent 圆点；默认 true。set false 让某些"行内 mini section"
  /// （如表单子区）少一点视觉噪音。
  dot = true,
  /// 默认底部 12px 间距；调用方在 flex 行内（与右侧 select 并排等场景）
  /// 不想要 margin 时传 noMargin。
  noMargin,
}: {
  children: React.ReactNode;
  subtitle?: React.ReactNode;
  right?: React.ReactNode;
  divider?: boolean;
  dot?: boolean;
  noMargin?: boolean;
}) {
  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        gap: 10,
        marginBottom: noMargin ? 0 : divider ? 10 : 12,
        paddingBottom: divider ? 10 : 0,
        // divider 不再用 1px solid 直拉到底；改用渐变 hairline，让 section
        // 切割感更克制，与 .pet-divider 节奏一致。
        ...(divider
          ? {
              backgroundImage:
                "linear-gradient(90deg, transparent, var(--pet-color-border) 12%, var(--pet-color-border) 88%, transparent)",
              backgroundRepeat: "no-repeat",
              backgroundSize: "100% 1px",
              backgroundPosition: "bottom",
            }
          : null),
      }}
    >
      {dot && (
        <span
          aria-hidden
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background:
              "radial-gradient(circle at 30% 30%, color-mix(in srgb, var(--pet-color-accent) 70%, white), var(--pet-color-accent))",
            boxShadow:
              "0 0 0 3px color-mix(in srgb, var(--pet-color-accent) 18%, transparent), 0 0 8px color-mix(in srgb, var(--pet-color-accent) 40%, transparent)",
            flexShrink: 0,
            alignSelf: "center",
          }}
        />
      )}
      <h3
        style={{
          margin: 0,
          fontSize: text.lg,
          fontWeight: fontWeight.semibold,
          color: "var(--pet-color-fg)",
          letterSpacing: letterSpacing.wider,
          lineHeight: lineHeight.tight,
        }}
      >
        {children}
      </h3>
      {subtitle && (
        <span
          style={{
            fontSize: text.sm,
            color: "var(--pet-color-muted)",
            letterSpacing: letterSpacing.wide,
            alignSelf: "baseline",
          }}
        >
          {subtitle}
        </span>
      )}
      {right && (
        <div
          style={{
            marginLeft: "auto",
            display: "flex",
            alignItems: "center",
            gap: 6,
          }}
        >
          {right}
        </div>
      )}
    </div>
  );
}
