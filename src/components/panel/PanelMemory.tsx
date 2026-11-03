import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TextArea } from "../ui/fields";
import { Button } from "../ui/Button";
import { StatusText } from "../ui/StatusText";
import { SettingsSection, SettingsRow } from "../ui/settings";
import { ExternalLinkIcon } from "../Icons";
import { useI18n, type TKey } from "../../i18n";
import { useStatusMessage } from "../../hooks/useStatusMessage";

/**
 * Per-agent memory editor: view and edit one agent's always-present markdown
 * files (SOUL.md / USER.md / MEMORY.md / HEARTBEAT.md, under `memory/<id>/`) and
 * open that agent's memory folder. Rendered inside the agent's settings section.
 *
 * The agent maintains USER.md / MEMORY.md / HEARTBEAT.md itself (during chat or on
 * a scheduled heartbeat), so a file can change while this is open. To avoid
 * clobbering those writes with stale content, edits save on blur ONLY when the
 * textarea actually differs from the last loaded/saved content, and focusing a
 * field re-reads the file first.
 */

type FieldKey = "soul" | "user" | "memory" | "heartbeat";

interface FieldDef {
  key: FieldKey;
  titleKey: TKey;
  rows: number;
  placeholderKey: TKey;
}

const FIELDS: FieldDef[] = [
  { key: "soul", titleKey: "memory.soul.title", rows: 6, placeholderKey: "memory.soul.placeholder" },
  { key: "user", titleKey: "memory.user.title", rows: 10, placeholderKey: "memory.user.placeholder" },
  { key: "memory", titleKey: "memory.memory.title", rows: 10, placeholderKey: "memory.memory.placeholder" },
  { key: "heartbeat", titleKey: "memory.heartbeat.title", rows: 8, placeholderKey: "memory.heartbeat.placeholder" },
];

const EMPTY: Record<FieldKey, string> = { soul: "", user: "", memory: "", heartbeat: "" };

export function AgentMemory({ agentId }: { agentId: string }) {
  const { t } = useI18n();
  const [values, setValues] = useState<Record<FieldKey, string>>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const status = useStatusMessage();
  // Last on-disk content we loaded or saved, per field. The dirty check and
  // focus-refresh both compare against this baseline.
  const baseline = useRef<Record<FieldKey, string>>({ ...EMPTY });

  useEffect(() => {
    Promise.all(FIELDS.map((f) => invoke<string>("get_memory_file", { agentId, kind: f.key })))
      .then((contents) => {
        const next = { ...EMPTY };
        FIELDS.forEach((f, i) => (next[f.key] = contents[i]));
        setValues(next);
        baseline.current = { ...next };
        setLoaded(true);
      })
      .catch((e) => {
        status.fail(t("common.loadFailed", { error: e }));
        setLoaded(true);
      });
  }, [agentId]);

  // Save only when the field changed since load/last-save, so opening it and
  // clicking through fields never overwrites memory the agent just wrote.
  const saveField = async (f: FieldDef, value: string) => {
    if (value === baseline.current[f.key]) return;
    try {
      await invoke("save_memory_file", { agentId, kind: f.key, content: value });
      baseline.current[f.key] = value;
      status.ok(t("common.saved"));
    } catch (e: any) {
      status.fail(t("common.saveFailed", { error: e }));
    }
  };

  // On focus, pull the latest on-disk content. If the file changed underneath
  // (e.g. the agent wrote to it) and the user has no unsaved local edits, adopt
  // the fresh content so they don't edit and then overwrite a stale version.
  const refreshField = async (f: FieldDef) => {
    try {
      const fresh = await invoke<string>("get_memory_file", { agentId, kind: f.key });
      setValues((prev) => {
        const unchangedOnDisk = fresh === baseline.current[f.key];
        const noLocalEdits = prev[f.key] === baseline.current[f.key];
        if (!unchangedOnDisk && noLocalEdits) {
          baseline.current[f.key] = fresh;
          return { ...prev, [f.key]: fresh };
        }
        return prev;
      });
    } catch {
      // Ignore refresh failures; keep showing the current content.
    }
  };

  const openMemoryDir = async () => {
    try {
      await invoke("open_dir", { kind: "memory", agentId });
    } catch (e: any) {
      status.fail(t("memory.openDirFailed", { error: e }));
    }
  };

  if (!loaded) {
    return <div className="py-4 text-center text-note text-ink-faint">{t("common.loading")}</div>;
  }

  return (
    <>
      <SettingsSection
        title={t("settings.agent.memoryTitle")}
        description={t("memory.subtitle")}
        action={
          <Button variant="ghost" size="sm" onClick={openMemoryDir} title={t("memory.openDir")}>
            <ExternalLinkIcon className="h-4 w-4" />
            {t("memory.openDirBtn")}
          </Button>
        }
      >
        {FIELDS.map((f) => (
          <SettingsRow key={f.key} label={t(f.titleKey)}>
            <TextArea
              value={values[f.key]}
              onChange={(e) => setValues((prev) => ({ ...prev, [f.key]: e.target.value }))}
              onFocus={() => refreshField(f)}
              onBlur={() => saveField(f, values[f.key])}
              rows={f.rows}
              placeholder={t(f.placeholderKey)}
            />
          </SettingsRow>
        ))}
      </SettingsSection>

      {status.message && (
        <StatusText ok={status.message.ok} className="text-note">{status.message.text}</StatusText>
      )}
    </>
  );
}