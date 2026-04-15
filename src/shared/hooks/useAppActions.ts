import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface UseAppActionsOptions {
  t: (key: string) => string;
  mqttEnabled: boolean;
  cloudSyncEnabled: boolean;
  openConfirm: (opts: { title: string; message: string; onConfirm: () => void }) => void;
  closeConfirm: () => void;
  pushToast: (msg: string, duration?: number) => number;
  fetchHistory: (reset?: boolean) => Promise<void>;
}

export const useAppActions = ({
  t,
  mqttEnabled,
  cloudSyncEnabled,
  openConfirm,
  closeConfirm,
  pushToast,
  fetchHistory
}: UseAppActionsOptions) => {
  const saveMqtt = useCallback(
    async (key: string, value: string) => {
      try {
        await invoke("save_setting", { key, value });
        // Restart MQTT client when any MQTT-related setting changes (if enabled)
        const mqttKeys = [
          "mqtt_enabled",
          "mqtt_server",
          "mqtt_port",
          "mqtt_username",
          "mqtt_password",
          "mqtt_topic",
          "mqtt_protocol",
          "mqtt_client_id"
        ];
        if (key === "mqtt_enabled" && value === "true") {
          await invoke("restart_mqtt_client");
        } else if (mqttKeys.includes(key) && mqttEnabled) {
          await invoke("restart_mqtt_client");
        }
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
      onConfirm: async () => {
        try {
          await invoke("clear_clipboard_history");
          fetchHistory(true);
        } catch (err) {
          console.error("清空失败", err);
        }
        closeConfirm();
      }
    });
  }, [closeConfirm, fetchHistory, openConfirm, t]);

  const handleResetSettings = useCallback(() => {
    openConfirm({
      title: t("reset_confirm"),
      message: t("reset_confirm"),
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
          "cloud_sync_webdav_base_path"
        ];
        if (key === "cloud_sync_enabled") {
          await invoke("restart_cloud_sync_client");
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
