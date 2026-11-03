import type { ReactNode } from "react";
import type { AppSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { TextInput, SavedTextInput, NumberField } from "../../ui/fields";
import { StatusText } from "../../ui/StatusText";
import { SettingsSection, SettingsRow } from "../../ui/settings";
import { ImageIcon, ExternalLinkIcon, DownloadIcon, SpinnerIcon } from "../../Icons";
import { useI18n } from "../../../i18n";

interface Props {
  form: AppSettings;
  /** Update the form in memory only (while typing). */
  onDraft: (next: AppSettings) => void;
  commitSettings: (next: AppSettings) => void;
  saveSettings: () => void;
  exampleBusy: boolean;
  onPickDirectory: (current: string, onPicked: (path: string) => void) => void;
  onOpenPath: (path: string) => void;
  onPickPetImageDir: () => void;
  onPickLive2DCore: () => void;
  onPickLive2DModel: () => void;
  onDownloadExample: () => void;
}

/** The pet's visual source: kind picker, Live2D files, gallery and image dirs. */
export function PetVisualCard({
  form,
  onDraft,
  commitSettings,
  saveSettings,
  exampleBusy,
  onPickDirectory,
  onOpenPath,
  onPickPetImageDir,
  onPickLive2DCore,
  onPickLive2DModel,
  onDownloadExample,
}: Props) {
  const { t } = useI18n();

  // Prerequisites for the two path-backed pet kinds. The pet-kind picker
  // disables those options until met, so the choice can never strand the pet
  // window on a kind with nothing to render.
  const live2dReady = !!form.live_2d_model_path.trim() && !!form.live_2d_core_path.trim();
  const galleryReady = !!form.gallery_dir.trim();

  return (
    <>
      <SettingsSection description={(!live2dReady || !galleryReady) ? t("settings.pet.gateHint") : undefined}>
        <SettingsRow>
          <div className="flex flex-wrap gap-1.5">
            <PetKindBtn
              active={form.pet_kind === "live2d"}
              disabled={!live2dReady}
              title={live2dReady ? "" : t("settings.pet.needLive2d")}
              onClick={() => commitSettings({ ...form, pet_kind: "live2d" })}
            >
              {t("settings.pet.kind.live2d")}
            </PetKindBtn>
            <PetKindBtn
              active={form.pet_kind === "gallery"}
              disabled={!galleryReady}
              title={galleryReady ? "" : t("settings.pet.needGallery")}
              onClick={() => commitSettings({ ...form, pet_kind: "gallery" })}
            >
              {t("settings.pet.kind.gallery")}
            </PetKindBtn>
            <PetKindBtn
              active={form.pet_kind === "image"}
              onClick={() => commitSettings({ ...form, pet_kind: "image" })}
            >
              {t("settings.pet.kind.image")}
            </PetKindBtn>
          </div>
        </SettingsRow>
        {form.pet_kind === "live2d" && !live2dReady && (
          <SettingsRow><StatusText ok={false} className="text-note">{t("settings.pet.needLive2d")}</StatusText></SettingsRow>
        )}
        {form.pet_kind === "gallery" && !galleryReady && (
          <SettingsRow><StatusText ok={false} className="text-note">{t("settings.pet.needGallery")}</StatusText></SettingsRow>
        )}
      </SettingsSection>

      <SettingsSection title={t("settings.live2d.title")}>
        <SettingsRow label={t("settings.live2d.corePath")} description={t("settings.live2d.corePathNote")}>
          <div className="flex gap-2">
            <SavedTextInput
              value={form.live_2d_core_path}
              onChange={(e) => onDraft({ ...form, live_2d_core_path: e.target.value })}
              onCommit={() => saveSettings()}
              className="flex-1"
              placeholder={t("settings.live2d.corePlaceholder")}
            />
            <Button variant="secondary" onClick={onPickLive2DCore}>
              <ImageIcon className="h-4 w-4" />
              {t("settings.gallery.pick")}
            </Button>
          </div>
        </SettingsRow>
        <SettingsRow label={t("settings.live2d.modelPath")} description={t("settings.live2d.modelPathNote")}>
          <div className="flex gap-2">
            <SavedTextInput
              value={form.live_2d_model_path}
              onChange={(e) => onDraft({ ...form, live_2d_model_path: e.target.value })}
              onCommit={() => saveSettings()}
              className="flex-1"
              placeholder={t("settings.live2d.modelPathPlaceholder")}
            />
            <Button variant="secondary" onClick={onPickLive2DModel}>
              <ImageIcon className="h-4 w-4" />
              {t("settings.gallery.pick")}
            </Button>
          </div>
        </SettingsRow>
        <SettingsRow
          label={t("settings.live2d.useExample")}
          align="start"
          control={
            <div className="group relative inline-flex">
              <Button variant="secondary" disabled={exampleBusy} onClick={onDownloadExample}>
                {exampleBusy ? (
                  <SpinnerIcon className="h-4 w-4 animate-spin" />
                ) : (
                  <DownloadIcon className="h-4 w-4" />
                )}
                {exampleBusy
                  ? t("settings.live2d.exampleDownloading")
                  : t("settings.live2d.useExample")}
              </Button>
              <div className="pointer-events-none invisible absolute bottom-full right-0 z-50 mb-2 w-80 rounded-field border border-line bg-surface p-3 text-note leading-relaxed text-ink-soft opacity-0 shadow-card transition-opacity duration-150 group-hover:visible group-hover:opacity-100">
                <div className="whitespace-pre-line [overflow-wrap:anywhere]">
                  {t("settings.live2d.exampleTooltip")}
                </div>
              </div>
            </div>
          }
        />
      </SettingsSection>

      <SettingsSection title={t("settings.gallery.title")}>
        <SettingsRow label={t("settings.gallery.dir")}>
          <div className="flex gap-2">
            <TextInput value={form.gallery_dir} readOnly className="flex-1" placeholder={t("settings.gallery.noDir")} />
            <Button variant="secondary" onClick={() => onPickDirectory(form.gallery_dir, (gallery_dir) => commitSettings({ ...form, gallery_dir }))}>
              <ImageIcon className="h-4 w-4" />
              {t("settings.gallery.pick")}
            </Button>
            <Button variant="secondary" onClick={() => onOpenPath(form.gallery_dir)} disabled={!form.gallery_dir} title={t("settings.gallery.openDirTitle")}>
              <ExternalLinkIcon className="h-4 w-4" />
              {t("common.open")}
            </Button>
          </div>
        </SettingsRow>
        <SettingsRow
          label={t("settings.gallery.interval")}
          description={t("settings.gallery.intervalNote")}
          control={
            <NumberField
              className="w-24"
              value={form.gallery_interval}
              fallback={10}
              onChange={(v) => onDraft({ ...form, gallery_interval: v })}
              onCommit={(v) => commitSettings({ ...form, gallery_interval: v })}
              placeholder="10"
            />
          }
        />
      </SettingsSection>

      <SettingsSection title={t("settings.pet.imageTitle")}>
        <SettingsRow label={t("settings.pet.imageDir")} description={t("settings.pet.imageDirNote")}>
          <div className="flex gap-2">
            <TextInput value={form.pet_image_dir} readOnly className="flex-1" placeholder={t("settings.pet.imageNoDir")} />
            <Button variant="secondary" onClick={onPickPetImageDir}>
              <ImageIcon className="h-4 w-4" />
              {t("settings.gallery.pick")}
            </Button>
            <Button variant="secondary" onClick={() => onOpenPath(form.pet_image_dir)} disabled={!form.pet_image_dir} title={t("settings.gallery.openDirTitle")}>
              <ExternalLinkIcon className="h-4 w-4" />
              {t("common.open")}
            </Button>
          </div>
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

/** One of the three pet-kind choices. Disabled (with the reason as `title`)
 *  while its prerequisite paths are unconfigured. */
function PetKindBtn({
  active,
  disabled,
  title,
  onClick,
  children,
}: {
  active: boolean;
  disabled?: boolean;
  title?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`rounded-full px-3.5 py-1.5 text-note font-medium transition-colors ${
        active
          ? "bg-accent text-white"
          : disabled
            ? "cursor-not-allowed bg-surface-soft text-ink-faint"
            : "bg-surface-soft text-ink-soft hover:bg-hover"
      }`}
    >
      {children}
    </button>
  );
}