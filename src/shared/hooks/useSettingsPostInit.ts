import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MutableRefObject } from "react";
import type { AiProfile, AppCleanupPolicy } from "../../features/settings/types";
import type { QuickPasteModifier } from "../../features/app/types";

const DEFAULT_AI_KEY = import.meta.env.VITE_AI_DEFAULT_API_KEY ?? "";
const AI_PRESET_IDS = new Set(["lc_flash_v1", "lc_think_v1", "lc_think_2601_v1"]);

const normalizeQuickPasteModifier = (value: string | undefined): QuickPasteModifier => {
  switch ((value || "").toLowerCase()) {
    case "disabled":
    case "ctrl":
    case "alt":
    case "shift":
    case "win":
      return value!.toLowerCase() as QuickPasteModifier;
    default:
      return "disabled";
  }
};

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
  setSensitiveMaskPrefixVisible: (val: number) => void;
  setSensitiveMaskSuffixVisible: (val: number) => void;
  setSensitiveMaskEmailDomain: (val: boolean) => void;
  setCleanupRules: (val: string) => void;
  setAppCleanupPolicies: (val: AppCleanupPolicy[]) => void;
  setSilentStart: (val: boolean) => void;
  setFollowMouse: (val: boolean) => void;
  setShowAppBorder: (val: boolean) => void;
  setShowSourceAppIcon: (val: boolean) => void;
  setDeleteAfterPaste: (val: boolean) => void;
  setMoveToTopAfterPaste: (val: boolean) => void;
  setHideTrayIcon: (val: boolean) => void;
  setEdgeDocking: (val: boolean) => void;
  setShowSearchBox: (val: boolean) => void;
  setScrollTopButtonEnabled: (val: boolean) => void;
  setArrowKeySelection: (val: boolean) => void;
  setMqttEnabled: (val: boolean) => void;
  setMqttServer: (val: string) => void;
  setRegistryWinVEnabled: (val: boolean) => void;
  setMqttPort: (val: string) => void;
  setMqttUser: (val: string) => void;
  setMqttPass: (val: string) => void;
  setMqttTopic: (val: string) => void;
  setMqttProtocol: (val: string) => void;
  setMqttWsPath: (val: string) => void;
  setMqttNotificationEnabled: (val: boolean) => void;
  setCloudSyncEnabled: (val: boolean) => void;
  setCloudSyncAuto: (val: boolean) => void;
  setCloudSyncProvider: (val: "http" | "webdav") => void;
  setCloudSyncServer: (val: string) => void;
  setCloudSyncApiKey: (val: string) => void;
  setCloudSyncIntervalSec: (val: string) => void;
  setCloudSyncSnapshotIntervalMin: (val: string) => void;
  setCloudSyncWebdavUrl: (val: string) => void;
  setCloudSyncWebdavUsername: (val: string) => void;
  setCloudSyncWebdavPassword: (val: string) => void;
  setCloudSyncWebdavBasePath: (val: string) => void;
  setFileServerAutoClose: (val: boolean) => void;
  setFileTransferAutoOpen: (val: boolean) => void;
  setFileTransferAutoCopy: (val: boolean) => void;
  setFileServerPort: (val: string) => void;
  setSequentialHotkey: (val: string) => void;
  setRichPasteHotkey: (val: string) => void;
  setSearchHotkey: (val: string) => void;
  setQuickPasteModifier: (val: QuickPasteModifier) => void;
  setSequentialModeState: (val: boolean) => void;
  setSoundEnabled: (val: boolean) => void;
  setSoundVolume: (val: number) => void;
  setPasteSoundEnabled: (val: boolean) => void;
  setPasteMethod: (val: string) => void;
  setAiEnabled: (val: boolean) => void;
  setAiTargetLang: (val: string) => void;
  setAiThinkingBudget: (val: string) => void;
  setIsWindowPinned: (val: boolean) => void;
  setAiProfiles: (val: AiProfile[]) => void;
  setAiAssignedProfileTask: (val: string) => void;
  setAiAssignedProfileMouthpiece: (val: string) => void;
  setAiAssignedProfileTranslate: (val: string) => void;
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
  setSensitiveMaskPrefixVisible,
  setSensitiveMaskSuffixVisible,
  setSensitiveMaskEmailDomain,
  setCleanupRules,
  setAppCleanupPolicies,
  setSilentStart,
  setFollowMouse,
  setShowAppBorder,
  setShowSourceAppIcon,
  setDeleteAfterPaste,
  setMoveToTopAfterPaste,
  setHideTrayIcon,
  setEdgeDocking,
  setShowSearchBox,
  setScrollTopButtonEnabled,
  setArrowKeySelection,
  setMqttEnabled,
  setMqttServer,
  setRegistryWinVEnabled,
  setMqttPort,
  setMqttUser,
  setMqttPass,
  setMqttTopic,
  setMqttProtocol,
  setMqttWsPath,
  setMqttNotificationEnabled,
  setCloudSyncEnabled,
  setCloudSyncAuto,
  setCloudSyncProvider,
  setCloudSyncServer,
  setCloudSyncApiKey,
  setCloudSyncIntervalSec,
  setCloudSyncSnapshotIntervalMin,
  setCloudSyncWebdavUrl,
  setCloudSyncWebdavUsername,
  setCloudSyncWebdavPassword,
  setCloudSyncWebdavBasePath,
  setFileServerAutoClose,
  setFileTransferAutoOpen,
  setFileTransferAutoCopy,
  setFileServerPort,
  setSequentialHotkey,
  setRichPasteHotkey,
  setSearchHotkey,
  setQuickPasteModifier,
  setSequentialModeState,
  setSoundEnabled,
  setSoundVolume,
  setPasteSoundEnabled,
  setPasteMethod,
  setAiEnabled,
  setAiTargetLang,
  setAiThinkingBudget,
  setIsWindowPinned,
  setAiProfiles,
  setAiAssignedProfileTask,
  setAiAssignedProfileMouthpiece,
  setAiAssignedProfileTranslate,
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

    // Theme application is centralized in the theme effect below
    if (settings["app.custom_background"]) setCustomBackground(settings["app.custom_background"]);
    if (settings["app.custom_background_opacity"]) {
      setCustomBackgroundOpacity(parseInt(settings["app.custom_background_opacity"]));
    }
    if (settings["app.surface_opacity"]) {
      const next = parseInt(settings["app.surface_opacity"]);
      if (Number.isFinite(next)) {
        setSurfaceOpacity(Math.min(100, Math.max(0, next)));
      }
    }
    if (settings["app.clipboard_item_font_size"]) {
      const next = parseInt(settings["app.clipboard_item_font_size"]);
      if (Number.isFinite(next)) setClipboardItemFontSize(next);
    }
    if (settings["app.clipboard_tag_font_size"]) {
      const next = parseInt(settings["app.clipboard_tag_font_size"]);
      if (Number.isFinite(next)) setClipboardTagFontSize(next);
    }
    if (settings["app.emoji_panel_enabled"] !== undefined) {
      setEmojiPanelEnabled(settings["app.emoji_panel_enabled"] === "true");
    }
    if (settings["app.tag_manager_enabled"] !== undefined) {
      setTagManagerEnabled(settings["app.tag_manager_enabled"] !== "false");
    }
    if (settings["app.emoji_panel_tab"] === "favorites" || settings["app.emoji_panel_tab"] === "emoji") {
      setEmojiPanelTab(settings["app.emoji_panel_tab"] as "emoji" | "favorites");
    }
    if (settings["app.emoji_favorites"]) {
      try {
        const parsed = JSON.parse(settings["app.emoji_favorites"]);
        if (Array.isArray(parsed)) {
          setEmojiFavorites(parsed.filter((p) => typeof p === "string"));
        }
      } catch (e) {
        console.warn("Invalid emoji favorites:", e);
      }
    }

    // Fix: explicitly handle both true and false cases for all boolean settings
    setPersistent(settings["app.persistent"] !== "false");
    setPersistentLimitEnabled(settings["app.persistent_limit_enabled"] !== "false");
    if (settings["app.persistent_limit"]) {
      setPersistentLimit(parseInt(settings["app.persistent_limit"]) || 1000);
    }
    setDeduplicate(settings["app.deduplicate"] !== "false");
    setCaptureFiles(settings["app.capture_files"] !== "false");
    setCaptureRichText(settings["app.capture_rich_text"] === "true");
    setRichTextSnapshotPreview(settings["app.rich_text_snapshot_preview"] === "true");
    setPrivacyProtection(settings["app.privacy_protection"] !== "false");
    if (settings["app.privacy_protection_kinds"]) {
      const list = settings["app.privacy_protection_kinds"]
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      if (list.length > 0) setPrivacyProtectionKinds(list);
    }
    if (settings["app.privacy_protection_custom_rules"] !== undefined) {
      setPrivacyProtectionCustomRules(settings["app.privacy_protection_custom_rules"] || "");
    }
    if (settings["app.sensitive_mask_prefix_visible"]) {
      const next = parseInt(settings["app.sensitive_mask_prefix_visible"]);
      if (Number.isFinite(next)) setSensitiveMaskPrefixVisible(Math.min(20, Math.max(0, next)));
    }
    if (settings["app.sensitive_mask_suffix_visible"]) {
      const next = parseInt(settings["app.sensitive_mask_suffix_visible"]);
      if (Number.isFinite(next)) setSensitiveMaskSuffixVisible(Math.min(20, Math.max(0, next)));
    }
    if (settings["app.sensitive_mask_email_domain"] !== undefined) {
      setSensitiveMaskEmailDomain(settings["app.sensitive_mask_email_domain"] === "true");
    }
    if (settings["app.cleanup_rules"] !== undefined) {
      setCleanupRules(settings["app.cleanup_rules"] || "");
    }
    if (settings["app.app_cleanup_policies"]) {
      try {
        const parsed = JSON.parse(settings["app.app_cleanup_policies"]);
        if (Array.isArray(parsed)) {
          setAppCleanupPolicies(
            parsed.filter(
              (item): item is AppCleanupPolicy =>
                !!item &&
                typeof item === "object" &&
                typeof item.id === "string" &&
                typeof item.enabled === "boolean" &&
                typeof item.appName === "string" &&
                typeof item.appPath === "string" &&
                (item.action === "ignore" || item.action === "clean") &&
                Array.isArray(item.contentTypes) &&
                typeof item.cleanupRules === "string"
            )
          );
        }
      } catch (e) {
        console.warn("Invalid app cleanup policies:", e);
      }
    }
    setSilentStart(settings["app.silent_start"] !== "false");
    setFollowMouse(settings["app.follow_mouse"] !== "false");
    setShowAppBorder(settings["app.show_app_border"] !== "false");
    setShowSourceAppIcon(settings["app.show_source_app_icon"] !== "false");

    // These have false as default, so check for 'true'
    setDeleteAfterPaste(settings["app.delete_after_paste"] === "true");
    setMoveToTopAfterPaste(settings["app.move_to_top_after_paste"] !== "false");
    setHideTrayIcon(settings["app.hide_tray_icon"] === "true");
    const edgeDockingEnabled = settings["app.edge_docking"] === "true";
    setEdgeDocking(edgeDockingEnabled);

    if (settings["app.show_search_box"] === "false") setShowSearchBox(false);
    setScrollTopButtonEnabled(settings["app.show_scroll_top_button"] !== "false");
    if (settings["app.arrow_key_selection"] === "false") setArrowKeySelection(false);

    setMqttEnabled(settings["mqtt_enabled"] === "true");
    setMqttServer(settings["mqtt_server"] || "");

    setRegistryWinVEnabled(settings["app.use_win_v_shortcut"] === "true");
    setMqttPort(settings["mqtt_port"] || "1883");
    setMqttUser(settings["mqtt_username"] || "");
    setMqttPass(settings["mqtt_password"] || "");
    const anonId = settings["app.anon_id"] || "";
    const shortId = anonId.split("-")[0] || "unknown";
    setMqttTopic(settings["mqtt_topic"] || `tiez/tiez_${shortId}`);
    setMqttProtocol(settings["mqtt_protocol"] || "mqtt://");
    setMqttWsPath(settings["mqtt_ws_path"] || "/mqtt");
    setMqttNotificationEnabled(settings["mqtt_notification_enabled"] !== "false");
    setCloudSyncEnabled(settings["cloud_sync_enabled"] === "true");
    setCloudSyncAuto(settings["cloud_sync_auto"] !== "false");
    setCloudSyncProvider(settings["cloud_sync_provider"] === "http" ? "http" : "webdav");
    setCloudSyncServer(settings["cloud_sync_server"] || "");
    setCloudSyncApiKey(settings["cloud_sync_api_key"] || "");
    setCloudSyncIntervalSec(settings["cloud_sync_interval_sec"] || "120");
    setCloudSyncSnapshotIntervalMin(settings["cloud_sync_snapshot_interval_min"] || "720");
    setCloudSyncWebdavUrl(settings["cloud_sync_webdav_url"] || "");
    setCloudSyncWebdavUsername(settings["cloud_sync_webdav_username"] || "");
    setCloudSyncWebdavPassword(settings["cloud_sync_webdav_password"] || "");
    setCloudSyncWebdavBasePath(settings["cloud_sync_webdav_base_path"] || "tiez-sync");
    setFileServerAutoClose(settings["file_transfer_auto_close"] === "true");
    setFileTransferAutoOpen(settings["file_transfer_auto_open"] === "true");
    setFileTransferAutoCopy(settings["file_transfer_auto_copy"] === "true");
    if (settings["file_server_port"]) setFileServerPort(settings["file_server_port"]);
    if (settings["app.sequential_hotkey"]) setSequentialHotkey(settings["app.sequential_hotkey"]);
    if (settings["app.rich_paste_hotkey"]) setRichPasteHotkey(settings["app.rich_paste_hotkey"]);
    if (settings["app.search_hotkey"] !== undefined) setSearchHotkey(settings["app.search_hotkey"]);
    setQuickPasteModifier(normalizeQuickPasteModifier(settings["app.quick_paste_modifier"]));
    if (settings["app.sequential_mode"] === "true") setSequentialModeState(true);
    if (settings["app.sound_enabled"] === "true") setSoundEnabled(true);
    if (settings["app.sound_volume"]) {
      const next = parseInt(settings["app.sound_volume"], 10);
      if (Number.isFinite(next)) {
        setSoundVolume(Math.min(100, Math.max(0, next)));
      }
    }
    setPasteSoundEnabled(settings["app.sound_paste_enabled"] !== "false");
    if (settings["app.paste_method"]) setPasteMethod(settings["app.paste_method"]);
    if (settings["ai_enabled"]) setAiEnabled(settings["ai_enabled"] === "true");
    if (settings["ai_target_lang"]) setAiTargetLang(settings["ai_target_lang"]);
    if (settings["ai_thinking_budget"]) setAiThinkingBudget(settings["ai_thinking_budget"]);

    if (settings["app.window_pinned"] === "true") {
      setIsWindowPinned(true);
      invoke("set_window_pinned", { pinned: true }).catch(console.error);
    }

    // 1. DEFINE PRESETS
    const recommended: AiProfile[] = [
      {
        id: "lc_flash_v1",
        baseUrl: "https://api.longcat.chat/openai/v1",
        apiKey: DEFAULT_AI_KEY,
        model: "LongCat-Flash-Chat",
        enableThinking: false
      },
      {
        id: "lc_think_v1",
        baseUrl: "https://api.longcat.chat/openai/v1",
        apiKey: DEFAULT_AI_KEY,
        model: "LongCat-Flash-Thinking",
        enableThinking: true
      },
      {
        id: "lc_think_2601_v1",
        baseUrl: "https://api.longcat.chat/openai/v1",
        apiKey: DEFAULT_AI_KEY,
        model: "LongCat-Flash-Thinking-2601",
        enableThinking: true
      }
    ];

    // 2. LOAD OR INIT
    let finalProfiles: AiProfile[] = recommended;
    if (settings["ai_profiles"]) {
      try {
        const parsed = JSON.parse(settings["ai_profiles"]);
        if (Array.isArray(parsed)) {
          finalProfiles = parsed
            .filter(
              (p): p is AiProfile =>
                !!p &&
                typeof p === "object" &&
                typeof p.id === "string" &&
                typeof p.baseUrl === "string" &&
                typeof p.apiKey === "string" &&
                typeof p.model === "string" &&
                typeof p.enableThinking === "boolean"
            )
            .map((profile) => {
              if (!DEFAULT_AI_KEY || !AI_PRESET_IDS.has(profile.id) || profile.apiKey.trim()) {
                return profile;
              }
              return {
                ...profile,
                apiKey: DEFAULT_AI_KEY,
              };
            });
        }
      } catch (e) {
        console.error(e);
      }
      if (DEFAULT_AI_KEY && JSON.stringify(finalProfiles) !== settings["ai_profiles"]) {
        invoke("save_setting", {
          key: "ai_profiles",
          value: JSON.stringify(finalProfiles)
        }).catch(console.error);
      }
    } else {
      // First time initialization
      invoke("save_setting", {
        key: "ai_profiles",
        value: JSON.stringify(recommended)
      }).catch(console.error);
    }
    setAiProfiles(finalProfiles);

    // 3. ASSIGNMENTS
    const getP = (m: string) =>
      finalProfiles.find((p) => p.model === m)?.id || finalProfiles[0]?.id || "default";

    setAiAssignedProfileTask(settings["ai_assigned_profile_task"] || getP("LongCat-Flash-Chat"));
    setAiAssignedProfileMouthpiece(
      settings["ai_assigned_profile_mouthpiece"] || getP("LongCat-Flash-Thinking-2601")
    );
    setAiAssignedProfileTranslate(
      settings["ai_assigned_profile_translate"] || getP("LongCat-Flash-Chat")
    );

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
    setSensitiveMaskPrefixVisible,
    setSensitiveMaskSuffixVisible,
    setSensitiveMaskEmailDomain,
    setCleanupRules,
    setAppCleanupPolicies,
    setSilentStart,
    setFollowMouse,
    setShowAppBorder,
    setShowSourceAppIcon,
    setDeleteAfterPaste,
    setMoveToTopAfterPaste,
    setHideTrayIcon,
    setEdgeDocking,
    setShowSearchBox,
    setScrollTopButtonEnabled,
    setArrowKeySelection,
    setMqttEnabled,
    setMqttServer,
    setRegistryWinVEnabled,
    setMqttPort,
    setMqttUser,
    setMqttPass,
    setMqttTopic,
    setMqttProtocol,
    setMqttWsPath,
    setCloudSyncEnabled,
    setCloudSyncAuto,
    setCloudSyncProvider,
    setCloudSyncServer,
    setCloudSyncApiKey,
    setCloudSyncIntervalSec,
    setCloudSyncSnapshotIntervalMin,
    setCloudSyncWebdavUrl,
    setCloudSyncWebdavUsername,
    setCloudSyncWebdavPassword,
    setCloudSyncWebdavBasePath,
    setFileServerAutoClose,
    setFileTransferAutoOpen,
    setFileTransferAutoCopy,
    setFileServerPort,
    setSequentialHotkey,
    setRichPasteHotkey,
    setSearchHotkey,
    setQuickPasteModifier,
    setSequentialModeState,
    setSoundEnabled,
    setSoundVolume,
    setPasteSoundEnabled,
    setPasteMethod,
    setAiEnabled,
    setAiTargetLang,
    setAiThinkingBudget,
    setIsWindowPinned,
    setAiProfiles,
    setAiAssignedProfileTask,
    setAiAssignedProfileMouthpiece,
    setAiAssignedProfileTranslate,
    setSettingsLoaded,
    setClipboardItemFontSize,
    setClipboardTagFontSize,
    setEmojiPanelEnabled,
    setTagManagerEnabled,
    setEmojiPanelTab,
    setEmojiFavorites
  ]);
};
