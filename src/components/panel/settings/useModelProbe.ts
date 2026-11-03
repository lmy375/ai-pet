import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ModelConfig } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";

export type ProviderOptions = {
  options: { id: string; label: string }[];
  /** What a request would actually use — equals `provider`, or genai's inference when it's empty. */
  resolved: string;
  /** Whether this protocol can express a numeric thinking budget. */
  renders_budget: boolean;
};

/**
 * Talks to the selected entry's endpoint: its model list, the protocols the
 * backend offers for that model, and the test-call button.
 */
export function useModelProbe(selected: string | null, config: ModelConfig | undefined) {
  const { t } = useI18n();
  const [models, setModels] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [providerOptions, setProviderOptions] = useState<ProviderOptions | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

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

  return { models, loadingModels, modelsError, providerOptions, testing, testResult, setTestResult, loadModels, handleTest };
}