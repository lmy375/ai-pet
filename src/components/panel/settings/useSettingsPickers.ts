import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { AppSettings, SkillsInfo } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import type { useStatusMessage } from "../../../hooks/useStatusMessage";

interface Args {
  form: AppSettings;
  setForm: (next: AppSettings) => void;
  saveSettings: (next?: AppSettings) => Promise<void>;
  commitSettings: (next: AppSettings) => void;
  status: ReturnType<typeof useStatusMessage>;
  skillsInfo: SkillsInfo | null;
  loadSkills: () => void;
}

/**
 * Native path pickers used across the pet / live2d / gallery / skills cards.
 * Each one persists its result through the caller's `commitSettings`.
 */
export function useSettingsPickers({ form, setForm, saveSettings, commitSettings, status, skillsInfo, loadSkills }: Args) {
  const { t } = useI18n();
  // "Use sample model" download in flight (one button, so one flag). Lives here,
  // not in the card, so switching tabs mid-download keeps the button busy.
  const [exampleBusy, setExampleBusy] = useState(false);

  /** Pick a directory via the native dialog, defaulting to `current` (or the
   *  OS Pictures folder), then persist immediately. */
  const pickDirectory = async (current: string, onPicked: (path: string) => void) => {
    try {
      const defaultPath = current || (await invoke<string | null>("default_gallery_dir").catch(() => null)) || undefined;
      const picked = await open({ directory: true, multiple: false, defaultPath });
      if (typeof picked === "string") onPicked(picked);
    } catch (e: any) {
      status.fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  const handleOpenPath = async (path: string) => {
    if (!path) return;
    try {
      await invoke("open_dir", { kind: "custom", path });
    } catch (e: any) {
      status.fail(t("settings.openGalleryDirFailed", { error: e }));
    }
  };

  /** Pick the image-pet directory and validate it before committing: every
   *  emotion (idle / thinking / happy) must have an image file, otherwise the
   *  choice is rejected — no silent fallback to the built-in art. */
  const handlePickPetImageDir = async () => {
    try {
      const defaultPath =
        form.pet_image_dir ||
        (await invoke<string | null>("default_gallery_dir").catch(() => null)) ||
        undefined;
      const picked = await open({ directory: true, multiple: false, defaultPath });
      if (typeof picked !== "string") return;

      const items = await invoke<{ path: string; kind: string }[]>("list_gallery_media", { dir: picked });
      const stems = new Set(
        items
          .filter((i) => i.kind === "image")
          .map((i) => i.path.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, "").toLowerCase()),
      );
      const missing = (["idle", "thinking", "happy"] as const).filter((e) => !stems.has(e));
      if (missing.length > 0) {
        status.fail(t("settings.pet.imageIncomplete", { missing: missing.join(", ") }));
        return;
      }
      commitSettings({ ...form, pet_image_dir: picked });
    } catch (e: any) {
      status.fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  // Persist a new skills dir and immediately re-scan it, so the list below the
  // input always reflects the directory shown in it.
  const commitSkillsDir = async (skills_dir: string) => {
    const next = { ...form, skills_dir };
    setForm(next);
    await saveSettings(next);
    loadSkills();
  };

  const handlePickSkillsDir = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: skillsInfo?.dir || undefined,
      });
      if (typeof picked === "string") commitSkillsDir(picked);
    } catch (e: any) {
      status.fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  const handleOpenConfigDir = async () => {
    try {
      await invoke("open_dir", { kind: "config" });
    } catch (e: any) {
      status.fail(t("settings.openConfigDirFailed", { error: e }));
    }
  };

  // Pick the Live2D core JS file (live2dcubismcore.min.js).
  // Empty = use the SDK bundled into the app.
  const handlePickLive2DCore = async () => {
    try {
      const defaultDir = await invoke<string>("default_live2d_dir").catch(() => null);
      const picked = await open({
        multiple: false,
        filters: [{ name: "JavaScript", extensions: ["js"] }],
        defaultPath: form.live_2d_core_path || defaultDir || undefined,
      });
      if (typeof picked === "string") commitSettings({ ...form, live_2d_core_path: picked });
    } catch (e: any) {
      status.fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  // Pick the Live2D model file (.model3.json).
  const handlePickLive2DModel = async () => {
    try {
      const defaultDir = await invoke<string>("default_live2d_dir").catch(() => null);
      const picked = await open({
        multiple: false,
        filters: [{ name: "Live2D Model", extensions: ["json"] }],
        defaultPath: form.live_2d_model_path || defaultDir || undefined,
      });
      if (typeof picked === "string") commitSettings({ ...form, live_2d_model_path: picked });
    } catch (e: any) {
      status.fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  // "Use sample model": download Live2D's official sample SDK + wanko model
  // (© Live2D Inc.) into <config>/live2d/ and wire both paths up in one go.
  // Rerunning re-downloads and overwrites, so it doubles as a repair.
  const handleDownloadExample = async () => {
    setExampleBusy(true);
    try {
      const paths = await invoke<{ core_path: string; model_path: string }>(
        "download_example_live2d",
      );
      commitSettings({
        ...form,
        live_2d_core_path: paths.core_path,
        live_2d_model_path: paths.model_path,
      });
      status.ok(t("settings.live2d.exampleDone"));
    } catch (e: any) {
      status.fail(t("settings.live2d.exampleFailed", { error: e }));
    } finally {
      setExampleBusy(false);
    }
  };

  return {
    exampleBusy,
    pickDirectory,
    handleOpenPath,
    handlePickPetImageDir,
    commitSkillsDir,
    handlePickSkillsDir,
    handleOpenConfigDir,
    handlePickLive2DCore,
    handlePickLive2DModel,
    handleDownloadExample,
  };
}