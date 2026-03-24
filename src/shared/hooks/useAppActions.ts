import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface UseAppActionsOptions {
  t: (key: string) => string;
  openConfirm: (opts: { title: string; message: string; onConfirm: () => void }) => void;
  closeConfirm: () => void;
  pushToast: (msg: string, duration?: number) => number;
  fetchHistory: (reset?: boolean) => Promise<void>;
}

export const useAppActions = ({
  t,
  openConfirm,
  closeConfirm,
  pushToast,
  fetchHistory
}: UseAppActionsOptions) => {
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

  return { clearHistory, handleResetSettings };
};
