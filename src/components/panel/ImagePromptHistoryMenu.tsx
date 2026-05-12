import { useEffect, useRef } from "react";
import type { ImagePromptEntry } from "./slashCommands";

interface Props {
  prompts: ImagePromptEntry[];
  selectedIdx: number;
  onSelect: (prompt: string) => void;
}

/// 输入框上方的浮窗 —— 列出最近 5 条 `/image` prompt（最新在前）。键盘上下 /
/// Enter 由父组件处理（与 SlashCommandMenu 行为对齐）；本组件只负责渲染 + 鼠
/// 标点击。空 prompts 数组由 caller 判断是否渲染（这里只兜底"没有历史"占位）。
export function ImagePromptHistoryMenu({ prompts, selectedIdx, onSelect }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const target = list.querySelector<HTMLDivElement>(
      `[data-prompt-idx="${selectedIdx}"]`,
    );
    if (target) {
      target.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIdx]);

  if (prompts.length === 0) {
    return null; // 没历史时父组件应该不渲染本组件；保险返回 null。
  }
  return (
    <div ref={listRef} style={menuContainerStyle}>
      <style>{`
        .pet-prompt-row {
          transition: background-color 0.12s ease;
        }
        .pet-prompt-row:hover {
          background: color-mix(in srgb, var(--pet-color-accent) 8%, transparent);
        }
      `}</style>
      <div
        style={{
          padding: "4px 12px",
          fontSize: "10px",
          color: "var(--pet-color-muted)",
          borderBottom: "1px solid var(--pet-color-border)",
          textTransform: "uppercase",
          letterSpacing: 0.5,
        }}
      >
        最近 prompt（↑↓ 选 · Enter 直发 · Tab 填回继续编辑）
      </div>
      {prompts.map((entry, i) => {
        const selected = i === selectedIdx;
        return (
          <div
            key={`${i}-${entry.prompt}`}
            className="pet-prompt-row"
            data-prompt-idx={i}
            onMouseDown={(e) => {
              // mousedown 而非 click 防 input blur 抢先（与 SlashCommandMenu 同）
              e.preventDefault();
              onSelect(entry.prompt);
            }}
            style={{
              padding: "6px 12px",
              cursor: "pointer",
              background: selected ? "var(--pet-tint-blue-bg)" : "transparent",
              borderLeft: selected
                ? "2px solid var(--pet-color-accent)"
                : "2px solid transparent",
              fontSize: "12px",
              color: selected
                ? "var(--pet-tint-blue-fg)"
                : "var(--pet-color-fg)",
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
            title={entry.prompt}
          >
            {/* 缩略图：有 thumb 显 24x24 圆角图，无 thumb fallback 🎨 emoji。
                同槽位让"有图 / 无图"行的对齐一致，文字起点不抖动。 */}
            {entry.thumb ? (
              <img
                src={entry.thumb}
                alt=""
                style={{
                  width: 24,
                  height: 24,
                  borderRadius: 4,
                  objectFit: "cover",
                  flexShrink: 0,
                  border: "1px solid var(--pet-color-border)",
                }}
              />
            ) : (
              <span
                style={{
                  width: 24,
                  height: 24,
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 14,
                  flexShrink: 0,
                }}
              >
                🎨
              </span>
            )}
            <span
              style={{
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                flex: 1,
                minWidth: 0,
              }}
            >
              {entry.prompt}
            </span>
          </div>
        );
      })}
    </div>
  );
}

const menuContainerStyle: React.CSSProperties = {
  position: "absolute",
  bottom: "100%",
  left: 0,
  right: 0,
  marginBottom: "6px",
  maxHeight: "240px",
  overflowY: "auto",
  background: "var(--pet-color-card)",
  border: "1px solid var(--pet-color-border)",
  borderRadius: "6px",
  boxShadow: "var(--pet-shadow-md)",
  zIndex: 20,
};
