import { useEffect } from "react";

interface UseSettingsPanelResetOptions {
  showSettings: boolean;
  setCollapsedGroups: (val: Record<string, boolean>) => void;
  setSettingsSubpage: (val: "home" | "advanced") => void;
}

export const useSettingsPanelReset = ({
  showSettings,
  setCollapsedGroups,
  setSettingsSubpage
}: UseSettingsPanelResetOptions) => {
  useEffect(() => {
    if (showSettings) {
      setSettingsSubpage("home");
      setCollapsedGroups({
        general: true,
        clipboard: true,
        appearance: true,
        sync: true,
        cloud_sync: true,
        ai: true,
        file_transfer: true,
        default_apps: true,
        data: true
      });
    }
  }, [showSettings, setCollapsedGroups, setSettingsSubpage]);
};
