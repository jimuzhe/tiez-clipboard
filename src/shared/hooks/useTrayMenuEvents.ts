import { useEffect, useRef } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauriRuntime";
import { CHECK_RESULT_EVENT, type UpdateCheckResult } from "./useAutoUpdate";

// Sent by the tray menu (src-tauri/src/app/tray.rs) after it has opened the window.
const OPEN_SETTINGS_EVENT = "tray-open-settings";
const CHECK_UPDATE_EVENT = "tray-check-update";

interface UseTrayMenuEventsOptions {
  t: (key: string) => string;
  pushToast: (msg: string, duration?: number) => number;
  openSettings: () => void;
}

/**
 * "Settings…" shows the settings page. "Check for Updates…" runs the same
 * check as the button in About: a found update opens the update dialog by
 * itself, every other outcome is reported in a toast.
 */
export const useTrayMenuEvents = ({ t, pushToast, openSettings }: UseTrayMenuEventsOptions) => {
  const latest = useRef({ t, pushToast, openSettings });
  latest.current = { t, pushToast, openSettings };

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const unlistenSettings = listen(OPEN_SETTINGS_EVENT, () => latest.current.openSettings());

    let unlistenResult: (() => void) | undefined;
    const unlistenUpdate = listen(CHECK_UPDATE_EVENT, async () => {
      const requestId = `tray-${Date.now()}`;
      unlistenResult?.();
      unlistenResult = await listen<UpdateCheckResult>(CHECK_RESULT_EVENT, (event) => {
        if (event.payload.requestId !== requestId) return;
        unlistenResult?.();
        unlistenResult = undefined;
        const { t: tr, pushToast: toast } = latest.current;
        const message: Record<UpdateCheckResult["status"], string | null> = {
          available: null,
          "up-to-date": tr("up_to_date"),
          error: tr("checking_failed"),
          portable: tr("portable_manual_update"),
        };
        const text = message[event.payload.status];
        if (text) toast(text, event.payload.status === "portable" ? 4000 : 2500);
      });
      await emit("check-update-manually", { requestId });
    });

    return () => {
      unlistenSettings.then((f) => f()).catch(() => {});
      unlistenUpdate.then((f) => f()).catch(() => {});
      unlistenResult?.();
    };
  }, []);
};
