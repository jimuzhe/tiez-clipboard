import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MutableRefObject } from "react";

interface UseSettingsPostInitOptions {
  settings: Record<string, string> | null;
  tagManagerSizeRef: MutableRefObject<{ width: number; height: number } | null>;
  setCustomBackground: (val: string) => void;
  setCustomBackgroundOpacity: (val: number) => void;
  setSurfaceOpacity: (val: number) => void;
  setPersistent: (val: boolean) => void;
  setPersistentLimitEnabled: (val: boolean) => void;
  setPersistentLimit: (val: number) => void;
  setDeduplicate: (val: boolean) => void;
  setCaptureFiles: (val: boolean) => void;
  setCaptureRichText: (val: boolean) => void;
  setRichTextSnapshotPreview: (val: boolean) => void;
  setPrivacyProtection: (val: boolean) => void;
  setPrivacyProtectionKinds: (val: string[]) => void;
  setPrivacyProtectionCustomRules: (val: string) => void;
  setSilentStart: (val: boolean) => void;
  setFollowMouse: (val: boolean) => void;
  setShowAppBorder: (val: boolean) => void;
  setDeleteAfterPaste: (val: boolean) => void;
  setMoveToTopAfterPaste: (val: boolean) => void;
  setHideTrayIcon: (val: boolean) => void;
  setEdgeDocking: (val: boolean) => void;
  setShowSearchBox: (val: boolean) => void;
  setScrollTopButtonEnabled: (val: boolean) => void;
  setArrowKeySelection: (val: boolean) => void;
  setRegistryWinVEnabled: (val: boolean) => void;
  setSequentialHotkey: (val: string) => void;
  setRichPasteHotkey: (val: string) => void;
  setSearchHotkey: (val: string) => void;
  setSequentialModeState: (val: boolean) => void;
  setSoundEnabled: (val: boolean) => void;
  setSoundVolume: (val: number) => void;
  setPasteSoundEnabled: (val: boolean) => void;
  setPasteMethod: (val: string) => void;
  setIsWindowPinned: (val: boolean) => void;
  setSettingsLoaded: (val: boolean) => void;
  setClipboardItemFontSize: (val: number) => void;
  setClipboardTagFontSize: (val: number) => void;
  setEmojiPanelEnabled: (val: boolean) => void;
  setTagManagerEnabled: (val: boolean) => void;
  setEmojiPanelTab: (val: "emoji" | "favorites") => void;
  setEmojiFavorites: (val: string[]) => void;
}

export const useSettingsPostInit = ({
  settings,
  tagManagerSizeRef,
  setCustomBackground,
  setCustomBackgroundOpacity,
  setSurfaceOpacity,
  setPersistent,
  setPersistentLimitEnabled,
  setPersistentLimit,
  setDeduplicate,
  setCaptureFiles,
  setCaptureRichText,
  setRichTextSnapshotPreview,
  setPrivacyProtection,
  setPrivacyProtectionKinds,
  setPrivacyProtectionCustomRules,
  setSilentStart,
  setFollowMouse,
  setShowAppBorder,
  setDeleteAfterPaste,
  setMoveToTopAfterPaste,
  setHideTrayIcon,
  setEdgeDocking,
  setShowSearchBox,
  setScrollTopButtonEnabled,
  setArrowKeySelection,
  setRegistryWinVEnabled,
  setSequentialHotkey,
  setRichPasteHotkey,
  setSearchHotkey,
  setSequentialModeState,
  setSoundEnabled,
  setSoundVolume,
  setPasteSoundEnabled,
  setPasteMethod,
  setIsWindowPinned,
  setSettingsLoaded,
  setClipboardItemFontSize,
  setClipboardTagFontSize,
  setEmojiPanelEnabled,
  setTagManagerEnabled,
  setEmojiPanelTab,
  setEmojiFavorites
}: UseSettingsPostInitOptions) => {
  useEffect(() => {
    if (!settings) return;

    if (settings["app.tag_manager_size"]) {
      try {
        const parsed = JSON.parse(settings["app.tag_manager_size"]);
        if (parsed && typeof parsed.width === "number" && typeof parsed.height === "number") {
          tagManagerSizeRef.current = { width: parsed.width, height: parsed.height };
        }
      } catch (e) {
        console.warn("Invalid tag manager size:", e);
      }
    }

    if (settings["app.custom_background"]) setCustomBackground(settings["app.custom_background"]);
    if (settings["app.custom_background_opacity"]) setCustomBackgroundOpacity(parseInt(settings["app.custom_background_opacity"]));
    if (settings["app.surface_opacity"]) {
      const next = parseInt(settings["app.surface_opacity"]);
      if (Number.isFinite(next)) setSurfaceOpacity(Math.min(100, Math.max(0, next)));
    }
    if (settings["app.clipboard_item_font_size"]) {
      const next = parseInt(settings["app.clipboard_item_font_size"]);
      if (Number.isFinite(next)) setClipboardItemFontSize(next);
    }
    if (settings["app.clipboard_tag_font_size"]) {
      const next = parseInt(settings["app.clipboard_tag_font_size"]);
      if (Number.isFinite(next)) setClipboardTagFontSize(next);
    }
    if (settings["app.emoji_panel_enabled"] !== undefined) setEmojiPanelEnabled(settings["app.emoji_panel_enabled"] === "true");
    if (settings["app.tag_manager_enabled"] !== undefined) setTagManagerEnabled(settings["app.tag_manager_enabled"] !== "false");
    if (settings["app.emoji_panel_tab"] === "favorites" || settings["app.emoji_panel_tab"] === "emoji") {
      setEmojiPanelTab(settings["app.emoji_panel_tab"] as "emoji" | "favorites");
    }
    if (settings["app.emoji_favorites"]) {
      try {
        const parsed = JSON.parse(settings["app.emoji_favorites"]);
        if (Array.isArray(parsed)) setEmojiFavorites(parsed.filter((p) => typeof p === "string"));
      } catch (e) {
        console.warn("Invalid emoji favorites:", e);
      }
    }

    setPersistent(settings["app.persistent"] !== "false");
    setPersistentLimitEnabled(settings["app.persistent_limit_enabled"] !== "false");
    if (settings["app.persistent_limit"]) setPersistentLimit(parseInt(settings["app.persistent_limit"]) || 1000);
    setDeduplicate(settings["app.deduplicate"] !== "false");
    setCaptureFiles(settings["app.capture_files"] !== "false");
    setCaptureRichText(settings["app.capture_rich_text"] === "true");
    setRichTextSnapshotPreview(settings["app.rich_text_snapshot_preview"] === "true");
    setPrivacyProtection(settings["app.privacy_protection"] !== "false");
    if (settings["app.privacy_protection_kinds"]) {
      const list = settings["app.privacy_protection_kinds"].split(",").map((s) => s.trim()).filter(Boolean);
      if (list.length > 0) setPrivacyProtectionKinds(list);
    }
    if (settings["app.privacy_protection_custom_rules"] !== undefined) setPrivacyProtectionCustomRules(settings["app.privacy_protection_custom_rules"] || "");
    setSilentStart(settings["app.silent_start"] !== "false");
    setFollowMouse(settings["app.follow_mouse"] !== "false");
    setShowAppBorder(settings["app.show_app_border"] !== "false");
    setDeleteAfterPaste(settings["app.delete_after_paste"] === "true");
    setMoveToTopAfterPaste(settings["app.move_to_top_after_paste"] !== "false");
    setHideTrayIcon(settings["app.hide_tray_icon"] === "true");
    setEdgeDocking(settings["app.edge_docking"] === "true");
    if (settings["app.show_search_box"] === "false") setShowSearchBox(false);
    setScrollTopButtonEnabled(settings["app.show_scroll_top_button"] !== "false");
    if (settings["app.arrow_key_selection"] === "false") setArrowKeySelection(false);
    setRegistryWinVEnabled(settings["app.use_win_v_shortcut"] === "true");

    if (settings["app.sequential_hotkey"]) setSequentialHotkey(settings["app.sequential_hotkey"]);
    if (settings["app.rich_paste_hotkey"]) setRichPasteHotkey(settings["app.rich_paste_hotkey"]);
    if (settings["app.search_hotkey"] !== undefined) setSearchHotkey(settings["app.search_hotkey"]);
    if (settings["app.sequential_mode"] === "true") setSequentialModeState(true);
    if (settings["app.sound_enabled"] === "true") setSoundEnabled(true);
    if (settings["app.sound_volume"]) {
      const next = parseInt(settings["app.sound_volume"], 10);
      if (Number.isFinite(next)) setSoundVolume(Math.min(100, Math.max(0, next)));
    }
    setPasteSoundEnabled(settings["app.sound_paste_enabled"] !== "false");
    if (settings["app.paste_method"]) setPasteMethod(settings["app.paste_method"]);

    if (settings["app.window_pinned"] === "true") {
      setIsWindowPinned(true);
      invoke("set_window_pinned", { pinned: true }).catch(console.error);
    }

    setSettingsLoaded(true);
  }, [
    settings,
    tagManagerSizeRef,
    setCustomBackground,
    setCustomBackgroundOpacity,
    setSurfaceOpacity,
    setPersistent,
    setPersistentLimitEnabled,
    setPersistentLimit,
    setDeduplicate,
    setCaptureFiles,
    setCaptureRichText,
    setRichTextSnapshotPreview,
    setPrivacyProtection,
    setPrivacyProtectionKinds,
    setPrivacyProtectionCustomRules,
    setSilentStart,
    setFollowMouse,
    setShowAppBorder,
    setDeleteAfterPaste,
    setMoveToTopAfterPaste,
    setHideTrayIcon,
    setEdgeDocking,
    setShowSearchBox,
    setScrollTopButtonEnabled,
    setArrowKeySelection,
    setRegistryWinVEnabled,
    setSequentialHotkey,
    setRichPasteHotkey,
    setSearchHotkey,
    setSequentialModeState,
    setSoundEnabled,
    setSoundVolume,
    setPasteSoundEnabled,
    setPasteMethod,
    setIsWindowPinned,
    setSettingsLoaded,
    setClipboardItemFontSize,
    setClipboardTagFontSize,
    setEmojiPanelEnabled,
    setTagManagerEnabled,
    setEmojiPanelTab,
    setEmojiFavorites
  ]);
};
