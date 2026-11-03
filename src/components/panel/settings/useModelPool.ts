import { useEffect, useState } from "react";
import type { AppSettings, ModelConfig } from "../../../hooks/useSettings";
import { newModel } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import { renameKey, uniqueName } from "./pool";

interface Args {
  settings: AppSettings;
  /** Update the form in memory only (while typing). */
  onDraft: (next: AppSettings) => void;
  /** Update and persist (discrete controls, blur, Enter). */
  onCommit: (next: AppSettings) => void;
  /** Report something the card did on its own (e.g. re-pointed agents). */
  notify: (text: string) => void;
}

/**
 * The selected entry of the global model pool plus every edit that changes the
 * pool itself (add / rename / duplicate / delete). Field edits stay in the card.
 */
export function useModelPool({ settings, onDraft, onCommit, notify }: Args) {
  const { t } = useI18n();
  const names = Object.keys(settings.models);
  const [selected, setSelectedState] = useState<string | null>(names[0] ?? null);
  const [nameDraft, setNameDraft] = useState(selected ?? "");

  const config: ModelConfig | undefined = selected ? settings.models[selected] : undefined;

  const select = (name: string) => {
    setSelectedState(name);
    setNameDraft(name);
  };

  // Follow the pool when the selected entry disappears (deleted, or the whole
  // config was reloaded from the raw YAML tab).
  //
  // The dependency is the joined key string on purpose: depending on `names`
  // (a new array every render) would re-run this after every keystroke.
  useEffect(() => {
    if (selected && settings.models[selected]) return;
    const next = names[0] ?? null;
    setSelectedState(next);
    setNameDraft(next ?? "");
  }, [names.join("\n")]);

  const withPool = (pool: Record<string, ModelConfig>): AppSettings => ({ ...settings, models: pool });

  const update = (updates: Partial<ModelConfig>, commit: boolean) => {
    if (!selected || !config) return;
    const next = withPool({ ...settings.models, [selected]: { ...config, ...updates } });
    (commit ? onCommit : onDraft)(next);
  };

  const add = async (from?: ModelConfig) => {
    const name = uniqueName(from ? `${selected} copy` : t("settings.models.newName"), names);
    onCommit(withPool({ ...settings.models, [name]: from ?? (await newModel()) }));
    setSelectedState(name);
    setNameDraft(name);
  };

  // Renaming the pool key re-points every agent that used the old name in the
  // same write — one edit, not a compatibility layer that reads both.
  const rename = (next: string) => {
    const name = next.trim();
    if (!selected || !name || name === selected) { setNameDraft(selected ?? ""); return; }
    if (settings.models[name]) { setNameDraft(selected); return; }
    onCommit({
      ...settings,
      models: renameKey(settings.models, selected, name),
      agents: settings.agents.map((a) => (a.model === selected ? { ...a, model: name } : a)),
    });
    setSelectedState(name);
    setNameDraft(name);
  };

  const usedBy = selected ? settings.agents.filter((a) => a.model === selected) : [];

  // Delete always goes through: the agents using this model are re-pointed at
  // whatever is left in the pool (and told so). Blocking the button instead left
  // it looking broken — WebKit shows no tooltip on a disabled control, so the
  // click just did nothing.
  const remove = () => {
    if (!selected) return;
    const { [selected]: _, ...rest } = settings.models;
    const fallback = Object.keys(rest)[0] ?? "";
    onCommit({
      ...settings,
      models: rest,
      agents: settings.agents.map((a) => (a.model === selected ? { ...a, model: fallback } : a)),
    });
    if (usedBy.length === 0) return;
    const agents = usedBy.map((a) => a.name).join("、");
    // Deleting the last model leaves them with nothing to switch to — say that
    // rather than reporting a move to "".
    notify(
      fallback
        ? t("settings.models.repointed", { agents, model: fallback })
        : t("settings.models.repointedNone", { agents })
    );
  };

  return { names, selected, select, nameDraft, setNameDraft, config, update, add, rename, remove, usedBy };
}