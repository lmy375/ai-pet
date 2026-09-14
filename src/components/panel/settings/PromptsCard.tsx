import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "../../ui/Card";
import { Button } from "../../ui/Button";
import { Badge } from "../../ui/Badge";
import { TextArea } from "../../ui/fields";
import { HintText } from "../../ui/feedback";
import { ExpandChevron, ExternalLinkIcon } from "../../Icons";
import { useI18n, type TKey } from "../../../i18n";

/** One system prompt, from the `list_prompts` command. */
export interface PromptInfo {
  key: string;
  /** Where an override lives (whether or not it exists yet). */
  path: string;
  /** True once an override file exists — this prompt no longer follows the app. */
  customized: boolean;
  /** The text currently in force (override, or the built-in default). */
  content: string;
  vars: string[];
  required_vars: string[];
}

interface Props {
  notify: (text: string) => void;
  fail: (text: string) => void;
}

/**
 * The system prompts the model reads before every turn. Editing one writes an
 * override file; until then the prompt follows the app and keeps improving with
 * it, which is why "restore default" (delete the file) is always one click away.
 */
export function PromptsCard({ notify, fail }: Props) {
  const { t } = useI18n();
  const [prompts, setPrompts] = useState<PromptInfo[] | null>(null);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  useEffect(() => {
    invoke<PromptInfo[]>("list_prompts")
      .then(setPrompts)
      .catch(() => setPrompts([]));
  }, []);

  /** Replace one entry in place, keeping the list order stable. */
  const put = (next: PromptInfo) =>
    setPrompts((prev) => (prev ?? []).map((p) => (p.key === next.key ? next : p)));

  const toggle = (p: PromptInfo) => {
    if (openKey === p.key) {
      setOpenKey(null);
      return;
    }
    setOpenKey(p.key);
    setDraft(p.content);
  };

  const save = async (key: string) => {
    try {
      const next = await invoke<PromptInfo>("save_prompt", { key, content: draft });
      put(next);
      notify(t("common.saved"));
    } catch (e: any) {
      fail(t("settings.prompts.saveFailed", { error: e }));
    }
  };

  const reset = async (key: string) => {
    try {
      const next = await invoke<PromptInfo>("reset_prompt", { key });
      put(next);
      setDraft(next.content);
      notify(t("settings.prompts.restored"));
    } catch (e: any) {
      fail(t("common.saveFailed", { error: e }));
    }
  };

  const openDir = async () => {
    try {
      await invoke("open_prompts_dir");
    } catch (e: any) {
      fail(t("settings.prompts.openDirFailed", { error: e }));
    }
  };

  return (
    <Card
      title={t("settings.prompts.title")}
      action={
        <Button variant="ghost" size="sm" onClick={openDir} title={t("settings.prompts.openDirTitle")}>
          <ExternalLinkIcon className="h-4 w-4" />
          {t("common.open")}
        </Button>
      }
    >
      <HintText className="mb-3">{t("settings.prompts.note")}</HintText>

      <div className="flex flex-col gap-1.5">
        {prompts?.map((p) => {
          const open = openKey === p.key;
          return (
            <div key={p.key} className="rounded-xl border border-line/70">
              <button
                onClick={() => toggle(p)}
                className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-hover"
              >
                <ExpandChevron expanded={open} className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
                <span className="text-body font-medium text-ink">
                  {t(`settings.prompts.name.${p.key}` as TKey)}
                </span>
                {p.customized ? (
                  <Badge color="amber">{t("settings.prompts.customized")}</Badge>
                ) : (
                  <Badge>{t("settings.prompts.default")}</Badge>
                )}
              </button>

              {open && (
                <div className="border-t border-line/70 px-3 py-3">
                  <TextArea
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    spellCheck={false}
                    className="min-h-[260px] font-mono !text-[12px]"
                  />
                  {p.vars.length > 0 && (
                    <HintText className="mt-2">
                      {t("settings.prompts.vars", {
                        vars: p.vars.map((v) => `{{${v}}}`).join(" "),
                        required: p.required_vars.map((v) => `{{${v}}}`).join(" ") || "—",
                      })}
                    </HintText>
                  )}
                  <div className="mt-2 flex items-center gap-2">
                    <Button size="sm" onClick={() => save(p.key)} disabled={draft === p.content}>
                      {t("settings.prompts.save")}
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => reset(p.key)}
                      disabled={!p.customized}
                      title={t("settings.prompts.restoreTitle")}
                    >
                      {t("settings.prompts.restore")}
                    </Button>
                    <span className="truncate font-mono text-[10px] text-ink-faint">{p.path}</span>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </Card>
  );
}
