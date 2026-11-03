import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, TelegramStatus } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import type { useStatusMessage } from "../../../hooks/useStatusMessage";

interface Args {
  status: ReturnType<typeof useStatusMessage>;
}

/** Live Telegram bot state for the agent being edited, plus its save+reconnect. */
export function useTelegramStatus({ status }: Args) {
  const { t } = useI18n();
  const [telegramStatus, setTelegramStatus] = useState<TelegramStatus>({ running: false, error: null });
  const [telegramReconnecting, setTelegramReconnecting] = useState(false);

  const loadTelegramStatus = (agentId: string) => {
    invoke<TelegramStatus>("get_telegram_status", { agentId })
      .then(setTelegramStatus)
      .catch(() => setTelegramStatus({ running: false, error: null }));
  };

  /** Save the current form, then ask the backend to (re)connect this agent's bot. */
  const reconnectTelegram = async (form: AppSettings, agentId: string) => {
    setTelegramReconnecting(true);
    status.clear();
    try {
      await invoke("save_settings", { settings: form });
      await invoke("reconnect_telegram");
      const tgStatus = await invoke<TelegramStatus>("get_telegram_status", { agentId });
      setTelegramStatus(tgStatus);
      if (tgStatus.running) status.ok(t("settings.tg.connected"));
      else status.ok(t("settings.tg.stoppedMsg"));
    } catch (e: any) {
      status.fail(t("settings.tg.opFailed", { error: e }));
    } finally {
      setTelegramReconnecting(false);
    }
  };

  return { telegramStatus, telegramReconnecting, setTelegramStatus, loadTelegramStatus, reconnectTelegram };
}