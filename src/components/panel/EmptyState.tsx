import { text, lineHeight, fontWeight } from "../../text";

/// 跨面板共享的"空态"提示：居中、有 icon 锚定视线、title + 可选 hint 两行。
/// 旧路径里散落的 `<div padding:12-24 textAlign:center color:muted>...</div>` 节奏
/// 不齐（padding 12 / 24 / 32，fontSize 11 / 12 / 13），看起来"想说话又没力气"。
/// 这里统一节奏 + 加 icon，让"空"也是一种主动的视觉表达。
///
/// 用法：
///   <EmptyState icon="📂" title="暂无历史会话" />
///   <EmptyState icon="🔍" title="没有匹配的消息" hint="试试不同关键词" />
///   <EmptyState icon="✅" title="任务清单是空的" hint="按上方「新建任务」开始" compact />
///
/// `compact`：padding 减半。给小区域（modal / inline list 末尾）用，避免空态
/// 撑得比内容区还大。
export function EmptyState({
  icon,
  title,
  hint,
  compact,
  children,
}: {
  icon?: string;
  title: string;
  hint?: string;
  compact?: boolean;
  /// 底部 action 区：用来挂"清除过滤"/"用范例预填" 等空态引导按钮。包在
  /// `marginTop: 14px` 容器里，与 hint 拉开节奏。空时不渲染容器。
  children?: React.ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        padding: compact ? "20px 12px" : "36px 16px",
        textAlign: "center",
        color: "var(--pet-color-muted)",
        userSelect: "none",
      }}
    >
      {icon && (
        <div
          aria-hidden
          style={{
            fontSize: compact ? 24 : 32,
            lineHeight: 1,
            opacity: 0.55,
            marginBottom: 2,
          }}
        >
          {icon}
        </div>
      )}
      <div
        style={{
          fontSize: compact ? text.base : text.md,
          fontWeight: fontWeight.medium,
          color: "var(--pet-color-fg)",
          opacity: 0.7,
        }}
      >
        {title}
      </div>
      {hint && (
        <div
          style={{
            fontSize: compact ? text.sm : text.base,
            color: "var(--pet-color-muted)",
            maxWidth: 260,
            lineHeight: lineHeight.base,
          }}
        >
          {hint}
        </div>
      )}
      {children && (
        <div
          style={{
            marginTop: 10,
            display: "flex",
            gap: 8,
            flexWrap: "wrap",
            justifyContent: "center",
          }}
        >
          {children}
        </div>
      )}
    </div>
  );
}
