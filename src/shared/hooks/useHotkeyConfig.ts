import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type HotkeyMode = "main" | "sequential" | "rich" | "search";

interface UseHotkeyConfigOptions {
  hotkey: string;
  setHotkey: (val: string) => void;
  sequentialHotkey: string;
  setSequentialHotkey: (val: string) => void;
  richPasteHotkey: string;
  setRichPasteHotkey: (val: string) => void;
  searchHotkey: string;
  setSearchHotkey: (val: string) => void;
  sequentialMode: boolean;
  isRecording: boolean;
  setIsRecording: (val: boolean) => void;
  isRecordingSequential: boolean;
  setIsRecordingSequential: (val: boolean) => void;
  isRecordingRich: boolean;
  setIsRecordingRich: (val: boolean) => void;
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
  richPasteHotkey,
  setRichPasteHotkey,
  searchHotkey,
  setSearchHotkey,
  sequentialMode,
  isRecording,
  setIsRecording,
  isRecordingSequential,
  setIsRecordingSequential,
  isRecordingRich,
  setIsRecordingRich,
  isRecordingSearch,
  setIsRecordingSearch,
  saveAppSetting,
  t,
  pushToast
}: UseHotkeyConfigOptions) => {
  const parseMainHotkeys = useCallback(
    (value: string): string[] =>
      value
        .split(/\r?\n/g)
        .map((item) => item.trim())
        .filter((item) => !!item),
    []
  );

  const serializeMainHotkeys = useCallback(
    (items: string[]): string =>
      items
        .map((item) => item.trim())
        .filter((item) => !!item)
        .join("\n"),
    []
  );

  const persistMainHotkeys = useCallback(
    async (items: string[]) => {
      const serialized = serializeMainHotkeys(items);
      setHotkey(serialized);
      saveAppSetting("hotkey", serialized);
      await invoke("register_hotkey", { hotkey: serialized }).catch((err) => {
        if (serialized) {
          const errorMsg = t("hotkey_register_failed") + (err?.toString() || "");
          pushToast(errorMsg, 3000);
        }
      });
    },
    [pushToast, saveAppSetting, serializeMainHotkeys, setHotkey, t]
  );

  const checkHotkeyConflict = useCallback(
    (newHotkey: string, mode: HotkeyMode): boolean => {
      if (!newHotkey) return false;
      const mainHotkeys = parseMainHotkeys(hotkey);

      const conflicts = [];
      if (mode !== "main" && mainHotkeys.includes(newHotkey)) conflicts.push(t("global_hotkey"));
      if (mode !== "sequential" && sequentialMode && newHotkey === sequentialHotkey) {
        conflicts.push(t("sequential_paste_hotkey_label"));
      }
      if (mode !== "rich" && newHotkey === richPasteHotkey) {
        conflicts.push(t("rich_paste_hotkey_label"));
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
    [hotkey, parseMainHotkeys, sequentialMode, sequentialHotkey, richPasteHotkey, searchHotkey, t, pushToast]
  );

  const updateHotkey = useCallback(
    async (newHotkey: string) => {
      const hasConflict = checkHotkeyConflict(newHotkey, "main");
      if (hasConflict) {
        setIsRecording(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || "快捷键被占用"}`;
          pushToast(errorMsg, 5000);
          setIsRecording(false);
          return;
        }
      }

      await persistMainHotkeys(newHotkey ? [newHotkey] : []);
      setIsRecording(false);
    },
    [checkHotkeyConflict, persistMainHotkeys, pushToast, setIsRecording]
  );

  const addMainHotkey = useCallback(
    async (newHotkey: string, options?: { skipAvailabilityCheck?: boolean }) => {
      const value = newHotkey.trim();
      if (!value) return;

      const hasConflict = checkHotkeyConflict(value, "main");
      if (hasConflict) {
        setIsRecording(false);
        return;
      }

      if (!options?.skipAvailabilityCheck) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: value });
        } catch (err) {
          const errorMsg = `❌ ${value}: ${err || "快捷键被占用"}`;
          pushToast(errorMsg, 5000);
          setIsRecording(false);
          return;
        }
      }

      const existing = parseMainHotkeys(hotkey);
      if (existing.includes(value)) {
        setIsRecording(false);
        return;
      }

      await persistMainHotkeys([...existing, value]);
      setIsRecording(false);
    },
    [checkHotkeyConflict, hotkey, parseMainHotkeys, persistMainHotkeys, pushToast, setIsRecording]
  );

  const removeMainHotkey = useCallback(
    async (targetHotkey: string) => {
      const existing = parseMainHotkeys(hotkey);
      const next = existing.filter((item) => item !== targetHotkey);
      if (next.length === existing.length) return;
      await persistMainHotkeys(next);
    },
    [hotkey, parseMainHotkeys, persistMainHotkeys]
  );

  const updateSequentialHotkey = useCallback(
    async (newHotkey: string) => {
      const hasConflict = checkHotkeyConflict(newHotkey, "sequential");
      if (hasConflict) {
        setIsRecordingSequential(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || "快捷键被占用"}`;
          pushToast(errorMsg, 5000);
          setIsRecordingSequential(false);
          return;
        }
      }

      setSequentialHotkey(newHotkey);
      saveAppSetting("sequential_hotkey", newHotkey);
      await invoke("set_sequential_hotkey", { hotkey: newHotkey }).catch(console.error);
      setIsRecordingSequential(false);
    },
    [
      checkHotkeyConflict,
      pushToast,
      saveAppSetting,
      setSequentialHotkey,
      setIsRecordingSequential
    ]
  );

  const updateRichPasteHotkey = useCallback(
    async (newHotkey: string) => {
      const hasConflict = checkHotkeyConflict(newHotkey, "rich");
      if (hasConflict) {
        setIsRecordingRich(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || "快捷键被占用"}`;
          pushToast(errorMsg, 5000);
          setIsRecordingRich(false);
          return;
        }
      }

      setRichPasteHotkey(newHotkey);
      saveAppSetting("rich_paste_hotkey", newHotkey);
      await invoke("set_rich_paste_hotkey", { hotkey: newHotkey }).catch(console.error);
      setIsRecordingRich(false);
    },
    [
      checkHotkeyConflict,
      pushToast,
      saveAppSetting,
      setRichPasteHotkey,
      setIsRecordingRich
    ]
  );

  const updateSearchHotkey = useCallback(
    async (newHotkey: string) => {
      const hasConflict = checkHotkeyConflict(newHotkey, "search");
      if (hasConflict) {
        setIsRecordingSearch(false);
        return;
      }

      if (newHotkey) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: newHotkey });
        } catch (err) {
          const errorMsg = `❌ ${newHotkey}: ${err || "快捷键被占用"}`;
          pushToast(errorMsg, 5000);
          setIsRecordingSearch(false);
          return;
        }
      }

      setSearchHotkey(newHotkey);
      saveAppSetting("search_hotkey", newHotkey);
      await invoke("set_search_hotkey", { hotkey: newHotkey }).catch(console.error);
      setIsRecordingSearch(false);
    },
    [
      checkHotkeyConflict,
      pushToast,
      saveAppSetting,
      setSearchHotkey,
      setIsRecordingSearch
    ]
  );

  useEffect(() => {
    invoke("set_recording_mode", {
      enabled: isRecording || isRecordingSequential || isRecordingRich
        || isRecordingSearch
    }).catch(console.error);

    if (isRecording || isRecordingSequential || isRecordingRich || isRecordingSearch) {
      const unlisten = listen<string>("hotkey-recorded", (event) => {
        if (isRecording) addMainHotkey(event.payload);
        if (isRecordingSequential) updateSequentialHotkey(event.payload);
        if (isRecordingRich) updateRichPasteHotkey(event.payload);
        if (isRecordingSearch) updateSearchHotkey(event.payload);
      });

      const unlistenCancel = listen("recording-cancelled", () => {
        setIsRecording(false);
        setIsRecordingSequential(false);
        setIsRecordingRich(false);
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
    isRecordingRich,
    isRecordingSearch,
    setIsRecording,
    setIsRecordingSequential,
    setIsRecordingRich,
    setIsRecordingSearch,
    addMainHotkey,
    updateSequentialHotkey,
    updateRichPasteHotkey,
    updateSearchHotkey
  ]);

  return {
    checkHotkeyConflict,
    updateHotkey,
    addMainHotkey,
    removeMainHotkey,
    updateSequentialHotkey,
    updateRichPasteHotkey,
    updateSearchHotkey
  };
};
