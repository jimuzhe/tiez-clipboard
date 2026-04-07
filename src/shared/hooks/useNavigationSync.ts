import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "../lib/tauriRuntime";

interface UseNavigationSyncOptions {
  showSettings: boolean;
  showTagManager: boolean;
  chatMode: boolean;
  showEmojiPanel: boolean;
}

export const useNavigationSync = ({
  showSettings,
  showTagManager,
  chatMode,
  showEmojiPanel
}: UseNavigationSyncOptions) => {
  useEffect(() => {
    if (!isTauriRuntime()) return;

    const shouldDisableNavigation = showSettings || showTagManager || chatMode || showEmojiPanel;
    if (shouldDisableNavigation) {
      invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
      return;
    }

    // Only enable global navigation when the window is actually visible.
    getCurrentWindow()
      .isVisible()
      .then((visible) => {
        invoke("set_navigation_enabled", { enabled: visible }).catch(console.error);
      })
      .catch(() => {
        invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
      });
  }, [showSettings, showTagManager, chatMode, showEmojiPanel]);
};
