import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, SkillsInfo } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { ErrorBox, HintText } from "../../ui/feedback";
import { SavedTextInput } from "../../ui/fields";
import { SettingsSection, SettingsRow } from "../../ui/settings";
import { ExternalLinkIcon } from "../../Icons";
import { useI18n } from "../../../i18n";

interface Props {
  form: AppSettings;
  skillsInfo: SkillsInfo | null;
  onDraft: (next: AppSettings) => void;
  loadSkills: () => void;
  commitSkillsDir: (dir: string) => void;
  onPickSkillsDir: () => void;
  fail: (text: string) => void;
}

/** The skills directory (with presets) and the skills scanned from it. */
export function SkillsPage({ form, skillsInfo, onDraft, loadSkills, commitSkillsDir, onPickSkillsDir, fail }: Props) {
  const { t } = useI18n();

  return (
    <>
      <SettingsSection
        title={t("settings.skills.title")}
        action={
          <button onClick={loadSkills} className="text-note font-medium text-accent hover:underline">
            {t("common.refresh")}
          </button>
        }
      >
        <SettingsRow label={t("settings.skills.dir")} description={t("settings.skills.dirNote", { dir: skillsInfo?.dir ?? "" })}>
          <div className="flex gap-2">
            <SavedTextInput
              value={form.skills_dir}
              onChange={(e) => onDraft({ ...form, skills_dir: e.target.value })}
              onCommit={() => commitSkillsDir(form.skills_dir)}
              className="flex-1"
              placeholder={skillsInfo?.dir ?? "~/.agents/skills"}
            />
            <Button variant="secondary" onClick={onPickSkillsDir}>
              {t("settings.skills.pick")}
            </Button>
            <Button variant="secondary" onClick={() => invoke("open_dir", { kind: "skills" }).catch((e: any) => fail(t("settings.skills.openDirFailed", { error: e })))} title={t("settings.skills.openDirTitle")}>
              <ExternalLinkIcon className="h-4 w-4" />
              {t("common.open")}
            </Button>
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {(skillsInfo?.presets ?? []).map((p) => (
              <button
                key={p}
                onClick={() => commitSkillsDir(p)}
                className="rounded-lg bg-surface-soft px-2 py-1 font-mono text-[11px] text-ink-soft transition-colors hover:bg-hover"
              >
                {p}
              </button>
            ))}
          </div>
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t("settings.skills.title")}>
        {skillsInfo?.skills.length ? (
          skillsInfo.skills.map((s) => (
            <SettingsRow
              key={s.path}
              label={<span className="flex items-baseline gap-2">{s.name}<span className="font-mono text-[11px] font-normal text-accent">/skill:{s.slug}</span></span>}
              description={s.error ? undefined : s.description}
            >
              {s.error ? (
                <ErrorBox className="mt-0">{s.error}</ErrorBox>
              ) : (
                <p className="truncate font-mono text-[10px] text-ink-faint">{s.path}</p>
              )}
            </SettingsRow>
          ))
        ) : (
          <SettingsRow><HintText className="mt-0">{t("settings.skills.empty")}</HintText></SettingsRow>
        )}
      </SettingsSection>
    </>
  );
}