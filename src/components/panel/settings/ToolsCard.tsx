import { Fragment, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../../ui/Button";
import { Badge } from "../../ui/Badge";
import { TextArea } from "../../ui/fields";
import { HintText } from "../../ui/feedback";
import { SettingsSection, SettingsRow, Switch } from "../../ui/settings";
import { ExpandChevron } from "../../Icons";
import { useI18n, type TKey } from "../../../i18n";

/** The context gate a built-in tool sits behind (`ToolScope` in the core). */
type ToolScope = "always" | "top_level" | "heartbeat" | "group" | "web_search";

/** One built-in tool, from the `list_tools` command. */
export interface ToolEntry {
  name: string;
  /** The description in force (the owner's rewrite if there is one). */
  description: string;
  default_description: string;
  customized: boolean;
  enabled: boolean;
  scope: ToolScope;
}

interface Props {
  notify: (text: string) => void;
  fail: (text: string) => void;
}

/**
 * The built-in tools. A switch here withholds the tool from every agent — the
 * model is not offered it and cannot call it — and the description below it is
 * the text the model reads to decide when to reach for the tool.
 *
 * Switching a tool on can't widen its reach: the badges name gates (heartbeat,
 * group, sub-agent depth, missing key) that still apply on top.
 */
export function ToolsCard({ notify, fail }: Props) {
  const { t } = useI18n();
  const [tools, setTools] = useState<ToolEntry[] | null>(null);
  const [openName, setOpenName] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const load = () =>
    invoke<ToolEntry[]>("list_tools")
      .then(setTools)
      .catch(() => setTools([]));

  useEffect(() => {
    load();
  }, []);

  const toggleOpen = (tool: ToolEntry) => {
    if (openName === tool.name) {
      setOpenName(null);
      return;
    }
    setOpenName(tool.name);
    setDraft(tool.description);
  };

  const setEnabled = async (name: string, enabled: boolean) => {
    setTools((prev) => (prev ?? []).map((t) => (t.name === name ? { ...t, enabled } : t)));
    try {
      await invoke("set_tool_enabled", { name, enabled });
      notify(enabled ? t("settings.tools.enabled", { name }) : t("settings.tools.disabled", { name }));
    } catch (e: any) {
      fail(t("common.saveFailed", { error: e }));
      load();
    }
  };

  const saveDescription = async (name: string) => {
    try {
      await invoke("save_tool_description", { name, content: draft });
      await load();
      notify(t("common.saved"));
    } catch (e: any) {
      fail(t("common.saveFailed", { error: e }));
    }
  };

  const resetDescription = async (tool: ToolEntry) => {
    try {
      await invoke("reset_tool_description", { name: tool.name });
      setDraft(tool.default_description);
      await load();
      notify(t("settings.prompts.restored"));
    } catch (e: any) {
      fail(t("common.saveFailed", { error: e }));
    }
  };

  return (
    <SettingsSection description={t("settings.tools.note")}>
      {tools?.map((tool) => {
        const open = openName === tool.name;
        return (
          <Fragment key={tool.name}>
            <SettingsRow
              label={
                <button onClick={() => toggleOpen(tool)} className="flex min-w-0 items-center gap-2 text-left">
                  <ExpandChevron expanded={open} className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
                  <span className={`font-mono text-body ${tool.enabled ? "text-ink" : "text-ink-faint line-through"}`}>
                    {tool.name}
                  </span>
                  {tool.scope !== "always" && (
                    <Badge color="sky">{t(`settings.tools.scope.${tool.scope}` as TKey)}</Badge>
                  )}
                  {tool.customized && <Badge color="amber">{t("settings.prompts.customized")}</Badge>}
                </button>
              }
              control={
                <Switch
                  checked={tool.enabled}
                  onChange={(on) => setEnabled(tool.name, on)}
                  title={t("settings.tools.switchTitle")}
                />
              }
            />

            {open && (
              <SettingsRow>
                <TextArea
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  spellCheck={false}
                  className="min-h-[160px] font-mono !text-[12px]"
                />
                <HintText className="mt-2">{t("settings.tools.descriptionNote")}</HintText>
                <div className="mt-2 flex items-center gap-2">
                  <Button
                    size="sm"
                    onClick={() => saveDescription(tool.name)}
                    disabled={draft === tool.description}
                  >
                    {t("settings.prompts.save")}
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => resetDescription(tool)}
                    disabled={!tool.customized}
                    title={t("settings.prompts.restoreTitle")}
                  >
                    {t("settings.prompts.restore")}
                  </Button>
                </div>
              </SettingsRow>
            )}
          </Fragment>
        );
      })}
    </SettingsSection>
  );
}