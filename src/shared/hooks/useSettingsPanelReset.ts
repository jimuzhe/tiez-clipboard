import { useEffect } from "react";

interface UseSettingsPanelResetOptions {
  showSettings: boolean;
  setCollapsedGroups: (val: Record<string, boolean>) => void;
}

export const useSettingsPanelReset = ({
  showSettings,
  setCollapsedGroups
}: UseSettingsPanelResetOptions) => {
  useEffect(() => {
    if (showSettings) {
      setCollapsedGroups({
        general: true,
        clipboard: true,
        appearance: true,
        default_apps: true,
        data: true
      });
    }
  }, [showSettings, setCollapsedGroups]);
};
