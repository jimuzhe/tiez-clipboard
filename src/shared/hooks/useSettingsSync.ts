import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../lib/tauriRuntime";

interface UseSettingsSyncOptions {
  settingsLoaded: boolean;
  deduplicate: boolean;
  saveAppSetting: (type: string, value: string) => void;
  captureFiles: boolean;
  captureRichText: boolean;
  fileTransferAutoCopy: boolean;
  fileServerAutoClose: boolean;
  fileTransferAutoOpen: boolean;
  persistent: boolean;
  arrowKeySelection: boolean;
  soundVolume: number;
  setIsKeyboardMode: (val: boolean) => void;
  setSelectedIndex: (val: number) => void;
}

export const useSettingsSync = ({
  settingsLoaded,
  deduplicate,
  saveAppSetting,
  captureFiles,
  captureRichText,
  fileTransferAutoCopy,
  fileServerAutoClose,
  fileTransferAutoOpen,
  persistent,
  arrowKeySelection,
  soundVolume,
  setIsKeyboardMode,
  setSelectedIndex
}: UseSettingsSyncOptions) => {
  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_deduplication", { enabled: deduplicate });
      saveAppSetting("deduplicate", String(deduplicate));
    }
  }, [deduplicate, saveAppSetting, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_capture_files", { enabled: captureFiles });
    }
  }, [captureFiles, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_capture_rich_text", { enabled: captureRichText });
    }
  }, [captureRichText, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_auto_copy_file", { enabled: fileTransferAutoCopy });
    }
  }, [fileTransferAutoCopy, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_file_server_auto_close", { enabled: fileServerAutoClose }).catch(console.error);
    }
  }, [fileServerAutoClose, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_file_transfer_auto_open", { enabled: fileTransferAutoOpen }).catch(console.error);
    }
  }, [fileTransferAutoOpen, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      invoke("set_persistence", { enabled: persistent });
    }
  }, [persistent, settingsLoaded]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      if (!arrowKeySelection) {
        setIsKeyboardMode(false);
        setSelectedIndex(0);
      }
      return;
    }

    invoke("set_arrow_key_selection", { enabled: arrowKeySelection }).catch(console.error);
    if (!arrowKeySelection) {
      setIsKeyboardMode(false);
      setSelectedIndex(0);
    }
  }, [arrowKeySelection, setIsKeyboardMode, setSelectedIndex]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (settingsLoaded) {
      saveAppSetting("sound_volume", String(soundVolume));
    }
  }, [soundVolume, saveAppSetting, settingsLoaded]);
};
