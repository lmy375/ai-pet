import { Button } from "../../ui/Button";
import { TextArea, Select } from "../../ui/fields";
import { SettingsSection, SettingsRow } from "../../ui/settings";
import { useI18n } from "../../../i18n";

/** Language switch — a plain form row, shared shape with the raw config page. */
export function LanguagePage({ value, onChange }: { value: string; onChange: (language: string) => void }) {
  const { t } = useI18n();
  return (
    <SettingsSection>
      <SettingsRow
        label={t("settings.language")}
        control={
          <Select
            className="w-40"
            value={value === "en" ? "en" : "zh"}
            onChange={onChange}
            options={[
              { value: "zh", label: "中文" },
              { value: "en", label: "English" },
            ]}
          />
        }
      />
    </SettingsSection>
  );
}

/** The raw `config.yaml` editor; saved on blur, re-read when the tab opens. */
export function RawConfigPage({
  value,
  onChange,
  onBlurSave,
  onOpenConfigDir,
}: {
  value: string;
  onChange: (value: string) => void;
  onBlurSave: () => void;
  onOpenConfigDir: () => void;
}) {
  const { t } = useI18n();
  return (
    <SettingsSection
      title="config.yaml"
      action={
        <Button variant="ghost" size="sm" onClick={onOpenConfigDir} title={t("settings.openConfigDirTitle")}>
          {t("settings.openConfigDir")}
        </Button>
      }
    >
      <SettingsRow>
        <TextArea
          autoGrow
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={onBlurSave}
          spellCheck={false}
          className="min-h-[360px] whitespace-pre font-mono !text-[12px] leading-relaxed"
        />
      </SettingsRow>
    </SettingsSection>
  );
}