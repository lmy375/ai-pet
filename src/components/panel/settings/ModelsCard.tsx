import { useEffect, useState } from "react";
import type { AppSettings, ModelConfig } from "../../../hooks/useSettings";
import { newModel } from "../../../hooks/useSettings";
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
import { useModelPool } from "./useModelPool";
import { useModelProbe } from "./useModelProbe";

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
  const pool = useModelPool({ settings, onDraft, onCommit, notify });
  const { names, selected, select, nameDraft, setNameDraft, config, update, add, rename, remove, usedBy } = pool;
  const probe = useModelProbe(selected, config);
  const { models, loadingModels, modelsError, providerOptions, testing, testResult, setTestResult, loadModels, handleTest } = probe;

  // Default values live on the Rust side; fetch the template once for the
  // "add" fallback and the placeholders (no local copy that could drift).
  const [tpl, setTpl] = useState<ModelConfig | null>(null);
  useEffect(() => {
    newModel().then(setTpl).catch((e) => console.error("Failed to load model template:", e));
  }, []);

  // Keep the configured id selectable even when the endpoint listing is absent
  // or stale, so opening this card can never blank out a working model.
  const modelOptions = config?.model && !models.includes(config.model) ? [config.model, ...models] : models;

  return (
    <SettingsSection>
      <SettingsRow>
        <ChipTabs
          items={names.map((name) => ({ name }))}
          selected={selected}
          onSelect={select}
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
                onChange={(provider) => { update({ provider }, true); setTestResult(null); }}
                options={(providerOptions?.options ?? [{ id: "", label: "Auto" }]).map((p) => ({
                  value: p.id,
                  label: p.label,
                }))}
              />
            }
          />

          <SettingsRow label="API Base URL">
            <SavedTextInput
              value={config.api_base}
              onChange={(e) => update({ api_base: e.target.value }, false)}
              onCommit={() => { update({}, true); loadModels(config.api_base, config.api_key, config.provider, config.model); }}
              placeholder={tpl?.api_base ?? ""}
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
              fallback={tpl?.context_window ?? 0}
              onChange={(v) => update({ context_window: v }, false)}
              onCommit={(v) => update({ context_window: v }, true)}
              placeholder={String(tpl?.context_window ?? 0)}
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
                onChange={(reasoning) => update({ reasoning: reasoning === "budget" ? "4096" : reasoning }, true)}
                options={[
                  { value: "", label: t("settings.llm.reasoningOff") },
                  { value: "minimal", label: "minimal" },
                  { value: "low", label: "low" },
                  { value: "medium", label: "medium" },
                  { value: "high", label: "high" },
                  { value: "xhigh", label: "xhigh" },
                  { value: "max", label: "max" },
                  { value: "budget", label: t("settings.llm.reasoningBudget") },
                ]}
              />
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