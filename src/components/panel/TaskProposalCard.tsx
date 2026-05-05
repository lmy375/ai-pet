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
      padding: 12,
      border: "1px solid #c7d2fe",
      borderRadius: 8,
      background: "linear-gradient(180deg, #eef2ff 0%, #f5f3ff 100%)",
      maxWidth: "90%",
    },
    head: { display: "flex", alignItems: "center", gap: 8, marginBottom: 6, fontSize: 12, color: "#4338ca", fontWeight: 600 },
    title: { fontSize: 14, fontWeight: 600, color: "#1e1b4b", marginBottom: 4 },
    body: { fontSize: 12, color: "#475569", lineHeight: 1.5, whiteSpace: "pre-wrap" as const, marginBottom: 6 },
    meta: { display: "flex", gap: 8, fontSize: 11, color: "#64748b", marginBottom: 10, flexWrap: "wrap" as const },
    priBadge: { padding: "2px 8px", borderRadius: 10, background: "#fef3c7", color: "#92400e" },
    dueBadge: { padding: "2px 8px", borderRadius: 10, background: "#e0e7ff", color: "#3730a3" },
    actions: { display: "flex", gap: 8 },
    btnPrimary: {
      padding: "6px 14px",
      border: "none",
      borderRadius: 6,
      background: "#6366f1",
      color: "#fff",
      cursor: "pointer",
      fontSize: 13,
    },
    btnSecondary: {
      padding: "6px 14px",
      border: "1px solid #c7d2fe",
      borderRadius: 6,
      background: "#fff",
      color: "#4338ca",
      cursor: "pointer",
      fontSize: 13,
    },
    btnDisabled: {
      padding: "6px 14px",
      border: "1px solid #e2e8f0",
      borderRadius: 6,
      background: "#f1f5f9",
      color: "#94a3b8",
      cursor: "not-allowed",
      fontSize: 13,
    },
    status: { fontSize: 12, color: "#475569" },
    err: { fontSize: 12, color: "#b91c1c", marginTop: 4 },
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
