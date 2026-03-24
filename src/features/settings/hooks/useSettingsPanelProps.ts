import type { ComponentProps } from "react";
import SettingsPanel from "../components/SettingsPanel";
import type { AppState } from "../../app/types";
import type { Locale } from "../../../shared/types";

type SettingsPanelProps = ComponentProps<typeof SettingsPanel>;

interface UseSettingsPanelPropsOptions {
  t: (key: string) => string;
  theme: string;
  language: Locale;
  colorMode: string;
  mainHotkeys: string[];
  checkHotkeyConflict: (newHotkey: string, mode: "main" | "sequential" | "rich" | "search") => boolean;
  updateHotkey: (key: string) => void;
  addMainHotkey: (key: string, options?: { skipAvailabilityCheck?: boolean }) => Promise<boolean>;
  removeMainHotkey: (key: string) => Promise<boolean>;
  updateSequentialHotkey: (key: string) => void;
  updateRichPasteHotkey: (key: string) => void;
  updateSearchHotkey: (key: string) => void;
  saveAppSetting: (key: string, val: string) => void;
  saveSetting: (key: string, val: string) => void;
  saveMqtt: (key: string, val: string) => void;
  saveCloudSync: (key: string, val: string) => void;
  fetchEffectiveTransferPath: () => void;
  handleResetSettings: () => void;
  toggleGroup: (group: string) => void;
  onOpenChat?: () => void;
  state: AppState;
}

export const useSettingsPanelProps = ({
  t,
  theme,
  language,
  colorMode,
  mainHotkeys,
  checkHotkeyConflict,
  updateHotkey,
  addMainHotkey,
  removeMainHotkey,
  updateSequentialHotkey,
  updateRichPasteHotkey,
  updateSearchHotkey,
  saveAppSetting,
  saveSetting,
  saveMqtt,
  saveCloudSync,
  fetchEffectiveTransferPath,
  handleResetSettings,
  toggleGroup,
  onOpenChat,
  state
}: UseSettingsPanelPropsOptions): SettingsPanelProps => {
  return {
    ...state,
    t,
    theme,
    language,
    colorMode,
    mainHotkeys,
    checkHotkeyConflict,
    updateHotkey,
    addMainHotkey,
    removeMainHotkey,
    updateSequentialHotkey,
    updateRichPasteHotkey,
    updateSearchHotkey,
    saveAppSetting,
    saveSetting,
    saveMqtt,
    saveCloudSync,
    fetchEffectiveTransferPath,
    handleResetSettings,
    toggleGroup,
    onOpenChat
  } as SettingsPanelProps;
};
