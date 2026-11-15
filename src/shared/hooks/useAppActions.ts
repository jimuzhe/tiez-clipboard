import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

interface UseAppActionsOptions {
  t: (key: string) => string;
  mqttEnabled: boolean;
  cloudSyncEnabled: boolean;
  openConfirm: (opts: { title: string; message: string; danger?: boolean; confirmLabel?: string; onConfirm: () => void }) => void;
  closeConfirm: () => void;
  pushToast: (msg: string, duration?: number) => number;
  fetchHistory: (reset?: boolean) => Promise<void>;
}

const MQTT_RESTART_DEBOUNCE_MS = 1200;

export const useAppActions = ({
  t,
  mqttEnabled,
  cloudSyncEnabled,
  openConfirm,
  closeConfirm,
  pushToast,
  fetchHistory
}: UseAppActionsOptions) => {
  // Typing in a connection field saves on every keystroke. Restart once the
  // user pauses instead of reconnecting per character (brokers may ban an IP
  // after a burst of failed logins); this also lets a protocol change and the
  // port change it brings land together.
  const mqttRestartTimer = useRef<number | null>(null);
  const saveMqtt = useCallback(
    async (key: string, value: string) => {
      try {
        await invoke("save_setting", { key, value });
        const mqttKeys = [
          "mqtt_enabled",
          "mqtt_server",
          "mqtt_port",
          "mqtt_username",
          "mqtt_password",
          "mqtt_topic",
          "mqtt_protocol",
          "mqtt_ws_path",
          "mqtt_client_id"
        ];
        const shouldRestart = (key === "mqtt_enabled" && value === "true") || (mqttKeys.includes(key) && mqttEnabled);
        if (!shouldRestart) return;
        if (mqttRestartTimer.current !== null) window.clearTimeout(mqttRestartTimer.current);
        const delay = key === "mqtt_enabled" ? 0 : MQTT_RESTART_DEBOUNCE_MS;
        mqttRestartTimer.current = window.setTimeout(() => {
          mqttRestartTimer.current = null;
          invoke("restart_mqtt_client").catch((err) => console.error("MQTT restart failed", err));
        }, delay);
      } catch (err) {
        console.error("MQTT Set save failed", err);
      }
    },
    [mqttEnabled]
  );

  const clearHistory = useCallback(() => {
    openConfirm({
      title: t("clear_confirm_title"),
      message: t("clear_confirm_message"),
      danger: true,
      confirmLabel: t("clear_confirm_action"),
      onConfirm: async () => {
        try {
          await invoke("clear_clipboard_history");
          fetchHistory(true);
        } catch (err) {
          console.error("清空失败", err);
          pushToast(t("clear_history_failed") + String(err), 4000);
        }
        closeConfirm();
      }
    });
  }, [closeConfirm, fetchHistory, openConfirm, pushToast, t]);

  const handleResetSettings = useCallback(() => {
    openConfirm({
      title: t("reset_defaults"),
      message: t("reset_confirm"),
      danger: true,
      confirmLabel: t("reset_confirm_action"),
      onConfirm: async () => {
        try {
          await invoke("reset_settings");
          window.location.reload();
        } catch (err) {
          const errorMsg = t("reset_failed") + (err?.toString() || "");
          pushToast(errorMsg, 3000);
        }
        closeConfirm();
      }
    });
  }, [closeConfirm, openConfirm, pushToast, t]);

  const saveCloudSync = useCallback(
    async (key: string, value: string) => {
      try {
        if (key === "cloud_sync_enabled" && value === "false") {
          await invoke("stop_cloud_sync_client");
        }
        await invoke("save_setting", { key, value });
        const cloudKeys = [
          "cloud_sync_enabled",
          "cloud_sync_auto",
          "cloud_sync_provider",
          "cloud_sync_server",
          "cloud_sync_api_key",
          "cloud_sync_interval_sec",
          "cloud_sync_snapshot_interval_min",
          "cloud_sync_webdav_url",
          "cloud_sync_webdav_username",
          "cloud_sync_webdav_password",
          "cloud_sync_webdav_base_path",
          "cloud_sync_content_prefs"
        ];
        if (key === "cloud_sync_enabled") {
          if (value !== "false") {
            await invoke("restart_cloud_sync_client");
          }
        } else if (cloudKeys.includes(key) && cloudSyncEnabled) {
          await invoke("restart_cloud_sync_client");
        }
      } catch (err) {
        console.error("Cloud sync setting save failed", err);
      }
    },
    [cloudSyncEnabled]
  );

  return { saveMqtt, saveCloudSync, clearHistory, handleResetSettings };
};
