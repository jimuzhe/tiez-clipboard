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
        .split(/[\r\n]+/g)
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

  const resolveHotkeyErrorMessage = useCallback(
    (err: unknown): string => {
      if (typeof err === "string" && err.trim()) return err;
      if (err instanceof Error && err.message.trim()) return err.message;
      const fallback = t("hotkey_unavailable");
      if (err === null || err === undefined) return fallback;
      const text = String(err).trim();
      return text || fallback;
    },
    [t]
  );

  const normalizeHotkeyForCompare = useCallback(
    (value: string): string =>
      value
        .split("+")
        .map((item) => item.trim().toUpperCase())
        .filter((item) => !!item)
        .join("+"),
    []
  );

  const checkHotkeyConflict = useCallback(
    (newHotkey: string, mode: HotkeyMode): boolean => {
      if (!newHotkey) return false;
      const normalizedNew = normalizeHotkeyForCompare(newHotkey);
      const mainHotkeys = parseMainHotkeys(hotkey);
      const normalizedMain = new Set(mainHotkeys.map(normalizeHotkeyForCompare));
      const normalizedSequential = normalizeHotkeyForCompare(sequentialHotkey);
      const normalizedRich = normalizeHotkeyForCompare(richPasteHotkey);
      const normalizedSearch = normalizeHotkeyForCompare(searchHotkey);

      const conflicts = [];
      if (mode !== "main" && normalizedMain.has(normalizedNew)) conflicts.push(t("global_hotkey"));
      if (mode !== "sequential" && sequentialMode && normalizedNew === normalizedSequential) {
        conflicts.push(t("sequential_paste_hotkey_label"));
      }
      if (mode !== "rich" && normalizedNew === normalizedRich) {
        conflicts.push(t("rich_paste_hotkey_label"));
      }
      if (mode !== "search" && normalizedNew === normalizedSearch) {
        conflicts.push(t("search_hotkey_label"));
      }

      if (conflicts.length > 0) {
        const msg = t("hotkey_conflict_toast").replace("{name}", conflicts[0]);
        pushToast(msg, 5000);
        return true;
      }
      return false;
    },
    [hotkey, normalizeHotkeyForCompare, parseMainHotkeys, sequentialMode, sequentialHotkey, richPasteHotkey, searchHotkey, t, pushToast]
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
          const errorMsg = `${newHotkey}: ${resolveHotkeyErrorMessage(err)}`;
          pushToast(errorMsg, 5000);
          setIsRecording(false);
          return;
        }
      }

      await persistMainHotkeys(newHotkey ? [newHotkey] : []);
      setIsRecording(false);
    },
    [checkHotkeyConflict, persistMainHotkeys, pushToast, resolveHotkeyErrorMessage, setIsRecording]
  );

  const addMainHotkey = useCallback(
    async (newHotkey: string, options?: { skipAvailabilityCheck?: boolean }) => {
      const value = newHotkey.trim();
      if (!value) return false;

      const hasConflict = checkHotkeyConflict(value, "main");
      if (hasConflict) {
        setIsRecording(false);
        return false;
      }

      if (!options?.skipAvailabilityCheck) {
        try {
          await invoke<boolean>("test_hotkey_available", { hotkey: value });
        } catch (err) {
          const errorMsg = `${value}: ${resolveHotkeyErrorMessage(err)}`;
          pushToast(errorMsg, 5000);
          setIsRecording(false);
          return false;
        }
      }

      const existing = parseMainHotkeys(hotkey);
      const normalizedValue = normalizeHotkeyForCompare(value);
      const hasExisting = existing.some((item) => normalizeHotkeyForCompare(item) === normalizedValue);
      if (hasExisting) {
        setIsRecording(false);
        return true;
      }

      await persistMainHotkeys([...existing, value]);
      setIsRecording(false);
      return true;
    },
    [checkHotkeyConflict, hotkey, normalizeHotkeyForCompare, parseMainHotkeys, persistMainHotkeys, pushToast, resolveHotkeyErrorMessage, setIsRecording]
  );

  const removeMainHotkey = useCallback(
    async (targetHotkey: string) => {
      const existing = parseMainHotkeys(hotkey);
      const normalizedTarget = normalizeHotkeyForCompare(targetHotkey);
      const next = existing.filter((item) => normalizeHotkeyForCompare(item) !== normalizedTarget);
      if (next.length === existing.length) return false;
      await persistMainHotkeys(next);
      return true;
    },
    [hotkey, normalizeHotkeyForCompare, parseMainHotkeys, persistMainHotkeys]
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
          const errorMsg = `${newHotkey}: ${resolveHotkeyErrorMessage(err)}`;
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
      setIsRecordingSequential,
      resolveHotkeyErrorMessage
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
          const errorMsg = `${newHotkey}: ${resolveHotkeyErrorMessage(err)}`;
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
      setIsRecordingRich,
      resolveHotkeyErrorMessage
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
          const errorMsg = `${newHotkey}: ${resolveHotkeyErrorMessage(err)}`;
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
      setIsRecordingSearch,
      resolveHotkeyErrorMessage
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
