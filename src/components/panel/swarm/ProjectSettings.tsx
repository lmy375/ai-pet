import { useEffect, useState } from "react";
import type { SwarmConfig, SwarmSnapshot } from "../../../hooks/useSwarm";
import type { AppSettings } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/fields";

export function ProjectSettings({
  snapshot,
  settings,
  onSave,
}: {
  snapshot: SwarmSnapshot;
  settings: AppSettings;
  onSave: (config: SwarmConfig) => Promise<void>;
}) {
  const { t } = useI18n();
  const [form, setForm] = useState<SwarmConfig>({
    title: snapshot.title,
    main_agent_id: snapshot.main_agent_id,
    pool: snapshot.pool,
    shared_prompt: snapshot.shared_prompt,
    orchestrator_prompt: snapshot.orchestrator_prompt,
    worker_prompt: snapshot.worker_prompt,
  });
  const [message, setMessage] = useState("");
  const locked = snapshot.status === "running" || snapshot.status === "draining";

  useEffect(() => {
    setForm({
      title: snapshot.title,
      main_agent_id: snapshot.main_agent_id,
      pool: snapshot.pool,
      shared_prompt: snapshot.shared_prompt,
      orchestrator_prompt: snapshot.orchestrator_prompt,
      worker_prompt: snapshot.worker_prompt,
    });
  }, [snapshot.id]);

  const updatePool = (agentId: string, count: number) => {
    setForm((prev) => ({
      ...prev,
      pool: [...prev.pool.filter((item) => item.agent_id !== agentId), { agent_id: agentId, count }],
    }));
  };

  const save = async () => {
    if (!form.title.trim()) {
      setMessage(t("swarm.project.nameRequired"));
      return;
    }
    setMessage("");
    try {
      await onSave({ ...form, title: form.title.trim(), pool: form.pool.filter((item) => item.count > 0) });
      setMessage(t("common.saved"));
    } catch (e) {
      setMessage(String(e));
    }
  };

  return (
    <div className="flex flex-col">
      <div className="sticky top-0 z-10 flex items-center justify-between gap-3 border-b border-line bg-surface/95 px-4 py-3 backdrop-blur">
        <span className="min-w-0 truncate text-note text-ink-soft">{locked ? t("swarm.settings.locked") : message}</span>
        <Button size="sm" onClick={save} disabled={locked}>{t("common.save")}</Button>
      </div>
      <div className="flex flex-col gap-5 p-4">
        <section>
          <div className="mb-2 text-note font-semibold text-ink">{t("swarm.settings.project")}</div>
          <label className="text-meta text-ink-faint">{t("swarm.project.name")}</label>
          <input
            value={form.title}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, title: e.target.value }))}
            className="mt-1 w-full rounded-field border border-line bg-surface px-3 py-2 text-body text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
          <label className="mt-3 block text-meta text-ink-faint">Orchestrator</label>
          <Select
            className="mt-1 w-full"
            value={form.main_agent_id}
            disabled={locked}
            onChange={(id) => setForm((prev) => ({ ...prev, main_agent_id: id }))}
            options={settings.agents.map((agent) => ({ value: agent.id, label: agent.name }))}
          />
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.workers")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.inherited")}</div>
          <div className="flex flex-col gap-2">
            {settings.agents.map((agent) => {
              const count = form.pool.find((item) => item.agent_id === agent.id)?.count ?? 0;
              const model = settings.models[agent.model]?.model ?? agent.model;
              return (
                <div key={agent.id} className="flex items-center gap-3 rounded-field border border-line bg-surface px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-body text-ink">{agent.name}</div>
                    <div className="truncate text-meta text-ink-faint">{model || t("chat.model.none")}</div>
                  </div>
                  <input
                    type="number"
                    min={0}
                    max={64}
                    disabled={locked}
                    value={count}
                    onChange={(e) => updatePool(agent.id, Number(e.target.value) || 0)}
                    className="w-16 rounded-field border border-line bg-surface-soft px-2 py-1 text-body text-ink outline-none disabled:text-ink-soft"
                  />
                </div>
              );
            })}
          </div>
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.sharedPrompt")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.sharedPromptHint")}</div>
          <textarea
            value={form.shared_prompt}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, shared_prompt: e.target.value }))}
            rows={6}
            className="w-full resize-y rounded-field border border-line bg-surface px-3 py-2 text-note text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.orchestratorPrompt")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.orchestratorPromptHint")}</div>
          <textarea
            value={form.orchestrator_prompt}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, orchestrator_prompt: e.target.value }))}
            rows={5}
            className="w-full resize-y rounded-field border border-line bg-surface px-3 py-2 text-note text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.workerPrompt")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.workerPromptHint")}</div>
          <textarea
            value={form.worker_prompt}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, worker_prompt: e.target.value }))}
            rows={5}
            className="w-full resize-y rounded-field border border-line bg-surface px-3 py-2 text-note text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
        </section>
      </div>
    </div>
  );
}