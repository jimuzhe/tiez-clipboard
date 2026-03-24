import { useEffect } from "react";

interface UseSettingsSyncOptions {
  settingsLoaded: boolean;
  deduplicate: boolean;
  saveAppSetting: (type: string, value: string) => void;
  saveSetting: (key: string, value: string) => void;
  captureFiles: boolean;
  captureRichText: boolean;
  fileTransferAutoCopy: boolean;
  fileServerAutoClose: boolean;
  fileTransferAutoOpen: boolean;
  persistent: boolean;
  soundVolume: number;
  arrowKeySelection: boolean;
  setIsKeyboardMode: (val: boolean) => void;
  setSelectedIndex: (val: number) => void;
}

export const useSettingsSync = ({
  settingsLoaded,
  deduplicate,
  saveAppSetting,
  saveSetting,
  captureFiles,
  captureRichText,
  fileTransferAutoCopy,
  fileServerAutoClose,
  fileTransferAutoOpen,
  persistent,
  soundVolume,
  arrowKeySelection,
  setIsKeyboardMode,
  setSelectedIndex
}: UseSettingsSyncOptions) => {
  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.deduplicate", String(deduplicate));
      saveAppSetting("deduplicate", String(deduplicate));
    }
  }, [deduplicate, saveAppSetting, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.capture_files", String(captureFiles));
    }
  }, [captureFiles, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.capture_rich_text", String(captureRichText));
    }
  }, [captureRichText, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.file_transfer_auto_copy", String(fileTransferAutoCopy));
    }
  }, [fileTransferAutoCopy, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.file_transfer_auto_close", String(fileServerAutoClose));
    }
  }, [fileServerAutoClose, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.file_transfer_auto_open", String(fileTransferAutoOpen));
    }
  }, [fileTransferAutoOpen, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveSetting("app.persistent", String(persistent));
    }
  }, [persistent, saveSetting, settingsLoaded]);

  useEffect(() => {
    if (settingsLoaded) {
      saveAppSetting("sound_volume", String(soundVolume));
    }
  }, [saveAppSetting, settingsLoaded, soundVolume]);

  useEffect(() => {
    saveSetting("app.arrow_key_selection", String(arrowKeySelection));
    if (!arrowKeySelection) {
      setIsKeyboardMode(false);
      setSelectedIndex(0);
    }
  }, [arrowKeySelection, saveSetting, setIsKeyboardMode, setSelectedIndex]);
};
