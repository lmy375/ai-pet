import { useState } from "react";
import type { AgentConfig } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/fields";
import { Modal } from "../../ui/Modal";
import { StatusText } from "../../ui/StatusText";
import { useStatusMessage } from "../../../hooks/useStatusMessage";

export function CreateProjectDialog({
  agents,
  defaultAgentId,
  onCreate,
  onClose,
}: {
  agents: AgentConfig[];
  defaultAgentId: string;
  onCreate: (title: string, orchestratorId: string) => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [orchestratorId, setOrchestratorId] = useState(defaultAgentId || agents[0]?.id || "");
  const [busy, setBusy] = useState(false);
  const status = useStatusMessage();

  const submit = async () => {
    if (!title.trim() || !orchestratorId || busy) return;
    setBusy(true);
    status.clear();
    try {
      await onCreate(title.trim(), orchestratorId);
    } catch (e) {
      status.fail(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      maxWidth={440}
      onClose={onClose}
      title={t("swarm.project.create")}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>{t("common.cancel")}</Button>
          <Button size="sm" onClick={submit} disabled={!title.trim() || !orchestratorId || busy}>{t("common.create")}</Button>
        </>
      }
    >
      <div className="mt-1 text-note text-ink-soft">{t("swarm.project.createHint")}</div>
      <label className="mt-4 block text-note font-medium text-ink">{t("swarm.project.name")}</label>
      <input
        autoFocus
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        placeholder={t("swarm.project.namePlaceholder")}
        className="mt-1 w-full rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
      />
      <label className="mt-4 block text-note font-medium text-ink">Orchestrator</label>
      <Select
        className="mt-1 w-full"
        value={orchestratorId}
        onChange={setOrchestratorId}
        options={agents.map((agent) => ({ value: agent.id, label: agent.name }))}
      />
      {status.message && (
        <StatusText ok={status.message.ok} className="mt-3 text-note">{status.message.text}</StatusText>
      )}
    </Modal>
  );
}

export function CreateTaskDialog({ onCreate, onClose }: { onCreate: (title: string, detail: string) => Promise<void>; onClose: () => void }) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [detail, setDetail] = useState("");
  const [busy, setBusy] = useState(false);
  const status = useStatusMessage();

  const submit = async () => {
    if (!title.trim() || !detail.trim() || busy) return;
    setBusy(true);
    status.clear();
    try {
      await onCreate(title.trim(), detail.trim());
    } catch (e) {
      status.fail(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal
      maxWidth={520}
      onClose={onClose}
      title={t("swarm.task.create")}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>{t("common.cancel")}</Button>
          <Button size="sm" onClick={submit} disabled={!title.trim() || !detail.trim() || busy}>{t("common.create")}</Button>
        </>
      }
    >
      <input
        autoFocus
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder={t("swarm.task.title")}
        className="mt-4 w-full rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
      />
      <textarea
        value={detail}
        onChange={(e) => setDetail(e.target.value)}
        placeholder={t("swarm.task.detail")}
        rows={7}
        className="mt-2 w-full resize-y rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
      />
      {status.message && (
        <StatusText ok={status.message.ok} className="mt-3 text-note">{status.message.text}</StatusText>
      )}
    </Modal>
  );
}