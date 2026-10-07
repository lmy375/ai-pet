import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, ModelConfig } from "../../../hooks/useSettings";
import { defaultModel } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { ChipTabs } from "../../ui/ChipTabs";
import { IconActionButton } from "../../ui/IconButton";
import { HintText } from "../../ui/feedback";
import { SavedTextInput, Select, NumberField } from "../../ui/fields";
import { SettingsSection, SettingsRow } from "../../ui/settings";
import { SearchSelect } from "../../ui/SearchSelect";
import { StatusText } from "../../ui/StatusText";
import { CopyIcon, TrashIcon } from "../../Icons";
import { useI18n } from "../../../i18n";
import { renameKey, uniqueName } from "./pool";

// Common model context windows, offered as one-tap presets next to the free
// numeric input (gpt-4o ~128K, Claude ~200K, Gemini ~1M).
const CONTEXT_PRESETS: { label: string; value: number }[] = [
  { label: "32K", value: 32000 },
  { label: "128K", value: 128000 },
  { label: "200K", value: 200000 },
  { label: "1M", value: 1000000 },
];

/** Effort keywords genai understands; anything else in `reasoning` is a token budget. */
const REASONING_KEYWORDS = ["", "none", "minimal", "low", "medium", "high", "xhigh", "max"];

type ProviderOptions = {
  options: { id: string; label: string }[];
  /** What a request would actually use — equals `provider`, or genai's inference when it's empty. */
  resolved: string;
  /** Whether this protocol can express a numeric thinking budget. */
  renders_budget: boolean;
};

interface Props {
  settings: AppSettings;
  /** Update the form in memory only (while typing). */
  onDraft: (next: AppSettings) => void;
  /** Update and persist (discrete controls, blur, Enter). */
  onCommit: (next: AppSettings) => void;
  /** Report something the card did on its own (e.g. re-pointed agents). */
  notify: (text: string) => void;
}

/**
 * The global model pool. Every agent picks one of these by name, so endpoint,
 * key, protocol, context window and reasoning effort are configured once here
 * instead of being copied into each agent.
 */
export function ModelsCard({ settings, onDraft, onCommit, notify }: Props) {
  const { t } = useI18n();
  const names = Object.keys(settings.models);
  const [selected, setSelected] = useState<string | null>(names[0] ?? null);
  const [nameDraft, setNameDraft] = useState(selected ?? "");
  const [models, setModels] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [providerOptions, setProviderOptions] = useState<ProviderOptions | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  const config: ModelConfig | undefined = selected ? settings.models[selected] : undefined;

  // Follow the pool when the selected entry disappears (deleted, or the whole
  // config was reloaded from the raw YAML tab).
  useEffect(() => {
    if (selected && settings.models[selected]) return;
    const next = names[0] ?? null;
    setSelected(next);
    setNameDraft(next ?? "");
  }, [names.join("\n")]);

  // Fetch the endpoint's model list when the selected entry changes. Failures
  // leave the list empty; the configured id stays selectable via the fallback
  // option below, so a copied entry never silently loses its model.
  useEffect(() => {
    setTestResult(null);
    if (config?.api_base.trim()) loadModels(config.api_base, config.api_key, config.provider, config.model);
    else { setModels([]); setModelsError(null); }
  }, [selected]);

  // Keep the protocol list — and what "Auto" resolves to for this model — in
  // sync. Resolution depends on the model name, so this re-runs when it changes.
  useEffect(() => {
    invoke<ProviderOptions>("list_providers", {
      model: config?.model ?? "",
      provider: config?.provider ?? "",
    })
      .then(setProviderOptions)
      .catch(() => setProviderOptions(null));
  }, [config?.model, config?.provider]);

  const loadModels = async (apiBase: string, apiKey: string, provider: string, model: string) => {
    if (!apiBase.trim()) return;
    setLoadingModels(true);
    setModelsError(null);
    try {
      // The provider decides which protocol the listing speaks — an Anthropic or
      // Gemini endpoint has no OpenAI-style /models route.
      setModels(await invoke<string[]>("list_models", { apiBase, apiKey, provider, model }));
    } catch (e: any) {
      setModels([]);
      setModelsError(String(e));
    } finally {
      setLoadingModels(false);
    }
  };

  /* ---------- Pool edits ---------- */

  const withPool = (pool: Record<string, ModelConfig>): AppSettings => ({ ...settings, models: pool });

  const update = (updates: Partial<ModelConfig>, commit: boolean) => {
    if (!selected || !config) return;
    const next = withPool({ ...settings.models, [selected]: { ...config, ...updates } });
    (commit ? onCommit : onDraft)(next);
  };

  const add = (from?: ModelConfig) => {
    const name = uniqueName(from ? `${selected} copy` : t("settings.models.newName"), names);
    onCommit(withPool({ ...settings.models, [name]: from ? { ...from } : defaultModel() }));
    setSelected(name);
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
    setSelected(name);
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

  const handleTest = async () => {
    if (!config) return;
    setTesting(true);
    setTestResult(null);
    try {
      await invoke("test_model", {
        apiBase: config.api_base,
        apiKey: config.api_key,
        model: config.model,
        provider: config.provider,
      });
      setTestResult({ ok: true, text: t("settings.llm.testOk") });
    } catch (e: any) {
      setTestResult({ ok: false, text: t("settings.llm.testFailed", { error: e }) });
    } finally {
      setTesting(false);
    }
  };

  // Keep the configured id selectable even when the endpoint listing is absent
  // or stale, so opening this card can never blank out a working model.
  const modelOptions = config?.model && !models.includes(config.model) ? [config.model, ...models] : models;

  return (
    <SettingsSection>
      <SettingsRow>
        <ChipTabs
          items={names.map((name) => ({ name }))}
          selected={selected}
          onSelect={(name) => { setSelected(name); setNameDraft(name); }}
          onAdd={() => add()}
          addTitle={t("settings.models.add")}
        />
      </SettingsRow>

      {!config ? (
        <SettingsRow>
          <HintText className="mt-0">{t("settings.models.empty")}</HintText>
        </SettingsRow>
      ) : (
        <>
          <SettingsRow label={t("settings.models.name")} description={t("settings.models.duplicateNote")}>
            <div className="flex gap-2">
              <SavedTextInput
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                onCommit={() => rename(nameDraft)}
                className="flex-1"
              />
              <IconActionButton size="sm" onClick={() => add(config)} title={t("settings.models.duplicate")}>
                <CopyIcon className="h-4 w-4" />
              </IconActionButton>
              <IconActionButton
                variant="danger"
                size="sm"
                onClick={remove}
                title={
                  usedBy.length > 0
                    ? t("settings.models.inUse", { agents: usedBy.map((a) => a.name).join("、") })
                    : t("common.delete")
                }
              >
                <TrashIcon className="h-4 w-4" />
              </IconActionButton>
            </div>
          </SettingsRow>

          <SettingsRow
            label={
              <span className="flex items-center gap-2">
                <span>{t("settings.llm.provider")}</span>
                {/* Auto-detection is a static model-name prefix map, so it guesses
                    wrong behind a gateway. Showing what it resolved to makes a bad
                    guess visible here instead of as a malformed request later. */}
                {!config.provider && providerOptions?.resolved && (
                  <span className="font-normal text-ink-faint">
                    {t("settings.llm.providerResolved", { provider: providerOptions.resolved })}
                  </span>
                )}
              </span>
            }
            description={t("settings.llm.providerHint")}
            control={
              <Select
                className="w-48"
                value={config.provider}
                onChange={(e) => { update({ provider: e.target.value }, true); setTestResult(null); }}
              >
                {(providerOptions?.options ?? [{ id: "", label: "Auto" }]).map((p) => (
                  <option key={p.id} value={p.id}>{p.label}</option>
                ))}
              </Select>
            }
          />

          <SettingsRow label="API Base URL">
            <SavedTextInput
              value={config.api_base}
              onChange={(e) => update({ api_base: e.target.value }, false)}
              onCommit={() => { update({}, true); loadModels(config.api_base, config.api_key, config.provider, config.model); }}
              placeholder={defaultModel().api_base}
            />
          </SettingsRow>

          <SettingsRow label="API Key">
            <SavedTextInput
              type="password"
              value={config.api_key}
              onChange={(e) => update({ api_key: e.target.value }, false)}
              onCommit={() => { update({}, true); loadModels(config.api_base, config.api_key, config.provider, config.model); }}
              placeholder="sk-..."
            />
          </SettingsRow>

          <SettingsRow
            label={
              <span className="flex items-center gap-2">
                <span>Model</span>
                {loadingModels && <span className="font-normal text-ink-faint">{t("common.loading")}</span>}
              </span>
            }
          >
            <div className="flex gap-2">
              <SearchSelect
                value={config.model}
                options={modelOptions}
                onChange={(m) => { update({ model: m }, true); setTestResult(null); }}
                disabled={modelOptions.length === 0}
                placeholder={
                  modelOptions.length === 0
                    ? (config.api_base.trim() ? t("settings.llm.noModelsHint") : t("settings.llm.fillBaseFirst"))
                    : t("settings.llm.selectFromN", { count: models.length })
                }
                emptyText={t("settings.llm.noMatch")}
                className="flex-1"
              />
              <Button onClick={handleTest} disabled={testing || !config.model.trim()}>
                {testing ? t("settings.llm.testing") : t("settings.llm.test")}
              </Button>
            </div>
            {modelsError && (
              <StatusText ok={false} className="mt-1.5 text-note">
                {t("settings.llm.modelsFailed", { error: modelsError })}
              </StatusText>
            )}
            {testResult && (
              <StatusText ok={testResult.ok} className="mt-1.5 text-note">{testResult.text}</StatusText>
            )}
          </SettingsRow>

          <SettingsRow label={t("settings.llm.contextWindow")} description={t("settings.llm.contextWindowNote")}>
            <div className="mb-2 flex gap-1.5">
              {CONTEXT_PRESETS.map((p) => (
                <button
                  key={p.value}
                  type="button"
                  onClick={() => update({ context_window: p.value }, true)}
                  className={`rounded-full px-2.5 py-1 text-note font-medium transition-colors ${
                    config.context_window === p.value ? "bg-accent text-white" : "bg-surface-soft text-ink-soft hover:bg-hover"
                  }`}
                >
                  {p.label}
                </button>
              ))}
            </div>
            <NumberField
              value={config.context_window}
              fallback={defaultModel().context_window}
              onChange={(v) => update({ context_window: v }, false)}
              onCommit={(v) => update({ context_window: v }, true)}
              placeholder={String(defaultModel().context_window)}
            />
          </SettingsRow>

          <SettingsRow
            label={t("settings.llm.reasoning")}
            description={t("settings.llm.reasoningNote")}
            align="start"
            control={
              <Select
                className="w-48"
                value={REASONING_KEYWORDS.includes(config.reasoning) ? config.reasoning : "budget"}
                onChange={(e) => update({ reasoning: e.target.value === "budget" ? "4096" : e.target.value }, true)}
              >
                <option value="">{t("settings.llm.reasoningOff")}</option>
                <option value="minimal">minimal</option>
                <option value="low">low</option>
                <option value="medium">medium</option>
                <option value="high">high</option>
                <option value="xhigh">xhigh</option>
                <option value="max">max</option>
                <option value="budget">{t("settings.llm.reasoningBudget")}</option>
              </Select>
            }
          />

          {!REASONING_KEYWORDS.includes(config.reasoning) && (
            <SettingsRow>
              <NumberField
                value={Number(config.reasoning) || 4096}
                min={1}
                fallback={4096}
                onChange={(v) => update({ reasoning: String(v) }, false)}
                onCommit={(v) => update({ reasoning: String(v) }, true)}
                placeholder="4096"
              />
              {/* The OpenAI protocol has no token-budget field, so this value
                  would go out as nothing at all. Say so here rather than
                  letting reasoning silently switch off. */}
              {providerOptions && !providerOptions.renders_budget && (
                <p className="mt-1 text-meta text-amber-600">
                  {t("settings.llm.reasoningBudgetUnsupported", { provider: providerOptions.resolved })}
                </p>
              )}
            </SettingsRow>
          )}
        </>
      )}
    </SettingsSection>
  );
}