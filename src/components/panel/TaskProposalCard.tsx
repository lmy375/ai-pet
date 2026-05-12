import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** 解析 `propose_task` 工具结果 JSON 后的提案对象。`due` 与后端 ISO
 * 形态对齐（`YYYY-MM-DDThh:mm`，无时区）。`null` 与字段缺失同义。*/
export interface TaskProposal {
  proposed: true;
  title: string;
  body: string;
  priority: number;
  due: string | null;
}

/** 解析 propose_task 工具结果。返回 null 表示不是有效提案（解析失败 /
 * 工具返回了 error），调用方应回退到普通 ToolCallBlock 展示，保留调试
 * 路径的可见性。*/
export function parseTaskProposal(result: string): TaskProposal | null {
  try {
    const parsed = JSON.parse(result);
    if (
      parsed &&
      typeof parsed === "object" &&
      parsed.proposed === true &&
      typeof parsed.title === "string" &&
      typeof parsed.priority === "number"
    ) {
      return {
        proposed: true,
        title: parsed.title,
        body: typeof parsed.body === "string" ? parsed.body : "",
        priority: parsed.priority,
        due: typeof parsed.due === "string" && parsed.due.length > 0 ? parsed.due : null,
      };
    }
  } catch {
    // 工具仍在 streaming 或返回了非 JSON —— 回退到普通展示
  }
  return null;
}

interface Props {
  proposal: TaskProposal;
}

type Phase = "pending" | "creating" | "created" | "cancelled" | "error";

function formatDue(iso: string | null): string {
  if (!iso) return "";
  return iso.replace("T", " ");
}

export function TaskProposalCard({ proposal }: Props) {
  const [phase, setPhase] = useState<Phase>("pending");
  const [errMsg, setErrMsg] = useState("");

  const handleConfirm = async () => {
    setPhase("creating");
    setErrMsg("");
    try {
      await invoke<string>("task_create", {
        args: {
          title: proposal.title,
          body: proposal.body,
          priority: proposal.priority,
          due: proposal.due,
        },
      });
      setPhase("created");
    } catch (e) {
      setPhase("error");
      setErrMsg(`${e}`);
    }
  };

  const handleCancel = () => {
    setPhase("cancelled");
  };

  const s = {
    card: {
      margin: "8px 0",
      padding: "14px 16px",
      border: "1px solid color-mix(in srgb, var(--pet-tint-purple-fg) 35%, transparent)",
      borderRadius: 10,
      background: "var(--pet-tint-purple-bg)",
      maxWidth: "90%",
      boxShadow: "var(--pet-shadow-sm)",
    },
    head: { display: "flex", alignItems: "center", gap: 8, marginBottom: 6, fontSize: 12, color: "var(--pet-tint-purple-fg)", fontWeight: 600, letterSpacing: 0.2 },
    title: { fontSize: 14, fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 4 },
    body: { fontSize: 12, color: "var(--pet-color-fg)", lineHeight: 1.55, whiteSpace: "pre-wrap" as const, marginBottom: 6 },
    meta: { display: "flex", gap: 8, fontSize: 11, color: "var(--pet-color-muted)", marginBottom: 10, flexWrap: "wrap" as const },
    priBadge: { padding: "2px 9px", borderRadius: 999, background: "var(--pet-tint-yellow-bg)", color: "var(--pet-tint-yellow-fg)", fontWeight: 600, letterSpacing: 0.3, border: "1px solid color-mix(in srgb, var(--pet-tint-yellow-fg) 18%, transparent)" },
    dueBadge: { padding: "2px 9px", borderRadius: 999, background: "var(--pet-tint-blue-bg)", color: "var(--pet-tint-blue-fg)", fontWeight: 600, letterSpacing: 0.3, border: "1px solid color-mix(in srgb, var(--pet-tint-blue-fg) 18%, transparent)" },
    actions: { display: "flex", gap: 8 },
    btnPrimary: {
      padding: "6px 14px",
      border: "none",
      borderRadius: 6,
      background: "var(--pet-tint-purple-fg)",
      color: "#fff",
      cursor: "pointer",
      fontSize: 13,
      fontWeight: 600,
      letterSpacing: 0.2,
    },
    btnSecondary: {
      padding: "6px 14px",
      border: "1px solid color-mix(in srgb, var(--pet-tint-purple-fg) 35%, transparent)",
      borderRadius: 6,
      background: "var(--pet-color-card)",
      color: "var(--pet-tint-purple-fg)",
      cursor: "pointer",
      fontSize: 13,
      fontWeight: 500,
    },
    btnDisabled: {
      padding: "6px 14px",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 6,
      background: "var(--pet-color-bg)",
      color: "var(--pet-color-muted)",
      cursor: "not-allowed",
      fontSize: 13,
    },
    status: { fontSize: 12, color: "var(--pet-color-muted)" },
    err: { fontSize: 12, color: "var(--pet-tint-red-fg)", marginTop: 4 },
  };

  return (
    <div style={s.card}>
      <div style={s.head}>📋 任务提案</div>
      <div style={s.title}>{proposal.title}</div>
      {proposal.body && <div style={s.body}>{proposal.body}</div>}
      <div style={s.meta}>
        <span style={s.priBadge}>P{proposal.priority}</span>
        {proposal.due && <span style={s.dueBadge}>截止 {formatDue(proposal.due)}</span>}
      </div>
      {phase === "pending" && (
        <div style={s.actions}>
          <button style={s.btnPrimary} onClick={handleConfirm}>
            创建任务
          </button>
          <button style={s.btnSecondary} onClick={handleCancel}>
            取消
          </button>
        </div>
      )}
      {phase === "creating" && (
        <div style={s.actions}>
          <button style={s.btnDisabled} disabled>
            创建中...
          </button>
        </div>
      )}
      {phase === "created" && <div style={s.status}>✅ 已加入队列</div>}
      {phase === "cancelled" && <div style={s.status}>已忽略</div>}
      {phase === "error" && (
        <>
          <div style={s.actions}>
            <button style={s.btnPrimary} onClick={handleConfirm}>
              重试
            </button>
            <button style={s.btnSecondary} onClick={handleCancel}>
              取消
            </button>
          </div>
          <div style={s.err}>创建失败：{errMsg}</div>
        </>
      )}
    </div>
  );
}
