import { useCallback, useEffect } from "react";
import { failureText } from "../lib/errorMessage";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauriRuntime";
import { isTypingHotkey } from "../lib/hotkeyKey";

type HotkeyMode = "main" | "sequential" | "plain" | "search";

interface UseHotkeyConfigOptions {
  hotkey: string;
  setHotkey: (val: string) => void;
  sequentialHotkey: string;
  setSequentialHotkey: (val: string) => void;
  plainPasteHotkey: string;
  setPlainPasteHotkey: (val: string) => void;
  searchHotkey: string;
  setSearchHotkey: (val: string) => void;
  sequentialMode: boolean;
  isRecording: boolean;
  setIsRecording: (val: boolean) => void;
  isRecordingSequential: boolean;
  setIsRecordingSequential: (val: boolean) => void;
  isRecordingPlain: boolean;
  setIsRecordingPlain: (val: boolean) => void;
  isRecordingSearch: boolean;
  setIsRecordingSearch: (val: boolean) => void;
  saveAppSetting: (type: string, value: string) => void;
  t: (key: string) => string;
  pushToast: (msg: string, duration?: number) => number;
}

export const useHotkeyConfig = ({
  hotkey,
  setHotkey,
  sequentialHotkey,
  setSequentialHotkey,
  plainPasteHotkey,
  setPlainPasteHotkey,
  searchHotkey,
  setSearchHotkey,
  sequentialMode,
  isRecording,
  setIsRecording,
  isRecordingSequential,
  setIsRecordingSequential,
  isRecordingPlain,
  setIsRecordingPlain,
  isRecordingSearch,
  setIsRecordingSearch,
  saveAppSetting,
  t,
  pushToast
}: UseHotkeyConfigOptions) => {
  const tauriReady = isTauriRuntime();

  const checkHotkeyConflict = useCallback(
    (newHotkey: string, mode: HotkeyMode): boolean => {
      if (!newHotkey) return false;
      if (isTypingHotkey(newHotkey)) {
        pushToast(t("hotkey_needs_modifier"), 5000);
        return true;
      }

      const conflicts = [];
      if (mode !== "main" && newHotkey === hotkey) conflicts.push(t("global_hotkey"));
      if (mode !== "sequential" && sequentialMode && newHotkey === sequentialHotkey) {
        conflicts.push(t("sequential_paste_hotkey_label"));
      }
      if (mode !== "plain" && newHotkey === plainPasteHotkey) {
        conflicts.push(t("plain_paste_hotkey_label"));
      }
      if (mode !== "search" && newHotkey === searchHotkey) {
        conflicts.push(t("search_hotkey_label"));
      }

      if (conflicts.length > 0) {
        const msg = t("hotkey_conflict_toast").replace("{name}", conflicts[0]);
        pushToast(msg, 5000);
        return true;
      }
      return false;
    },
    [hotkey, sequentialMode, sequentialHotkey, plainPasteHotkey, searchHotkey, t, pushToast]
  );

  const updateHotkey = useCallback(
    async (newHotkey: string) => {
      if (!tauriReady) {
        setHotkey(newHotkey);
        saveAppSetting("hotkey", newHotkey);
        setIsRecording(false);
        return;
      }

      const hasConflict = checkHotkeyConflict(newHotkey, "main");
      if (hasConflict) {
        setIsRecording(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || t("hotkey_occupied")}`;
          pushToast(errorMsg, 5000);
          setIsRecording(false);
          return;
        }
      }

      setHotkey(newHotkey);
      saveAppSetting("hotkey", newHotkey);
      await invoke("register_hotkey", { hotkey: newHotkey }).catch((err) => {
        if (newHotkey) {
          const errorMsg = failureText(t("hotkey_register_failed"), err);
          pushToast(errorMsg, 3000);
        }
      });
      setIsRecording(false);
    },
    [checkHotkeyConflict, pushToast, saveAppSetting, setHotkey, setIsRecording, t, tauriReady]
  );

  const updateSequentialHotkey = useCallback(
    async (newHotkey: string) => {
      if (!tauriReady) {
        setSequentialHotkey(newHotkey);
        saveAppSetting("sequential_hotkey", newHotkey);
        setIsRecordingSequential(false);
        return;
      }

      const hasConflict = checkHotkeyConflict(newHotkey, "sequential");
      if (hasConflict) {
        setIsRecordingSequential(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || t("hotkey_occupied")}`;
          pushToast(errorMsg, 5000);
          setIsRecordingSequential(false);
          return;
        }
      }

      setSequentialHotkey(newHotkey);
      saveAppSetting("sequential_hotkey", newHotkey);
      await invoke("set_sequential_hotkey", { hotkey: newHotkey }).catch((err) => {
        console.error(err);
        if (newHotkey) pushToast(failureText(t("hotkey_register_failed"), err), 3000);
      });
      setIsRecordingSequential(false);
    },
    [
      checkHotkeyConflict,
      pushToast,
      saveAppSetting,
      setSequentialHotkey,
      setIsRecordingSequential,
      t,
      tauriReady
    ]
  );

  const updatePlainPasteHotkey = useCallback(
    async (newHotkey: string) => {
      if (!tauriReady) {
        setPlainPasteHotkey(newHotkey);
        saveAppSetting("plain_paste_hotkey", newHotkey);
        setIsRecordingPlain(false);
        return;
      }

      const hasConflict = checkHotkeyConflict(newHotkey, "plain");
      if (hasConflict) {
        setIsRecordingPlain(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || t("hotkey_occupied")}`;
          pushToast(errorMsg, 5000);
          setIsRecordingPlain(false);
          return;
        }
      }

      setPlainPasteHotkey(newHotkey);
      saveAppSetting("plain_paste_hotkey", newHotkey);
      await invoke("set_plain_paste_hotkey", { hotkey: newHotkey }).catch((err) => {
        console.error(err);
        if (newHotkey) pushToast(failureText(t("hotkey_register_failed"), err), 3000);
      });
      setIsRecordingPlain(false);
    },
    [
      checkHotkeyConflict,
      pushToast,
      saveAppSetting,
      setPlainPasteHotkey,
      setIsRecordingPlain,
      t,
      tauriReady
    ]
  );

  const updateSearchHotkey = useCallback(
    async (newHotkey: string) => {
      if (!tauriReady) {
        setSearchHotkey(newHotkey);
        saveAppSetting("search_hotkey", newHotkey);
        setIsRecordingSearch(false);
        return;
      }

      const hasConflict = checkHotkeyConflict(newHotkey, "search");
      if (hasConflict) {
        setIsRecordingSearch(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || t("hotkey_occupied")}`;
          pushToast(errorMsg, 5000);
          setIsRecordingSearch(false);
          return;
        }
      }

      setSearchHotkey(newHotkey);
      saveAppSetting("search_hotkey", newHotkey);
      await invoke("set_search_hotkey", { hotkey: newHotkey }).catch((err) => {
        console.error(err);
        if (newHotkey) pushToast(failureText(t("hotkey_register_failed"), err), 3000);
      });
      setIsRecordingSearch(false);
    },
    [
      checkHotkeyConflict,
      pushToast,
      saveAppSetting,
      setSearchHotkey,
      setIsRecordingSearch,
      t,
      tauriReady
    ]
  );

  useEffect(() => {
    if (!tauriReady) return;

    invoke("set_recording_mode", {
      enabled: isRecording || isRecordingSequential || isRecordingPlain || isRecordingSearch
    }).catch(console.error);

    if (isRecording || isRecordingSequential || isRecordingPlain || isRecordingSearch) {
      const unlisten = listen<string>("hotkey-recorded", (event) => {
        if (isRecording) updateHotkey(event.payload);
        if (isRecordingSequential) updateSequentialHotkey(event.payload);
        if (isRecordingPlain) updatePlainPasteHotkey(event.payload);
        if (isRecordingSearch) updateSearchHotkey(event.payload);
      });

      const unlistenCancel = listen("recording-cancelled", () => {
        setIsRecording(false);
        setIsRecordingSequential(false);
        setIsRecordingPlain(false);
        setIsRecordingSearch(false);
      });

      return () => {
        unlisten.then((f) => f());
        unlistenCancel.then((f) => f());
      };
    }
  }, [
    isRecording,
    isRecordingSequential,
    isRecordingPlain,
    isRecordingSearch,
    setIsRecording,
    setIsRecordingSequential,
    setIsRecordingPlain,
    setIsRecordingSearch,
    updateHotkey,
    updateSequentialHotkey,
    updatePlainPasteHotkey,
    updateSearchHotkey,
    tauriReady
  ]);

  return {
    checkHotkeyConflict,
    updateHotkey,
    updateSequentialHotkey,
    updatePlainPasteHotkey,
    updateSearchHotkey
  };
};
