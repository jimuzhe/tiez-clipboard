
import { memo, useState, useEffect, useCallback, useRef, type ReactNode } from "react";
import type { ClickPasteMode } from "../../../shared/lib/clickPaste";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import {
    AppWindow,
    ChevronRight,
    Clipboard,
    Cloud,
    Database,
    Info,
    ListFilter,
    MessageSquareText,
    Palette,
    Settings2,
    ShieldCheck,
    Sparkles,
    Wifi,
    type LucideIcon
} from "lucide-react";
import type { Locale } from "../../../shared/types";
import { failureText, formatError } from "../../../shared/lib/errorMessage";
import { syncErrorText } from "../../../shared/lib/syncError";
import type { DefaultAppsMap, InstalledAppOption, SettingsSubpage, CloudSyncContentPrefs } from "../../app/types";
import type { AiProfile, AiProfileStatusMap, AppCleanupPolicy, EditableAiProfile } from "../types";
import { resolveTaskProfile } from "../aiPresets";
import { chosenApp, unchosenFallback } from "../utils/defaultApps";
import AppSelectorModal from "./AppSelectorModal";
// Removed UpdateModal imports
import AiProfileModal from "./AiProfileModal";
import GeneralSettingsGroup from "./groups/GeneralSettingsGroup";
import ClipboardSettingsGroup from "./groups/ClipboardSettingsGroup";
import AdvancedSettingsGroup from "./groups/AdvancedSettingsGroup";
import AppearanceSettingsGroup from "./groups/AppearanceSettingsGroup";
import SyncSettingsGroup from "./groups/SyncSettingsGroup";
import CloudSyncSettingsGroup, { type CloudSyncStatusPayload } from "./groups/CloudSyncSettingsGroup";
import { useE2eKey } from "../hooks/useE2eKey";
import DefaultAppsSettingsGroup from "./groups/DefaultAppsSettingsGroup";
import DataSettingsGroup from "./groups/DataSettingsGroup";
import FileTransferSettingsGroup from "./groups/FileTransferSettingsGroup";
import { useUnreadableSecrets } from "../useUnreadableSecrets";
import AiSettingsGroup from "./groups/AiSettingsGroup";
import SettingsFooter from "./SettingsFooter";
import { LabelWithHint, SettingsHintProvider } from "./SettingsHint";
import SecureTransfer from "./SecureTransfer";
import { useWideWindow } from "../../../shared/hooks/useWideWindow";
import { usePaneWidth } from "../../../shared/hooks/usePaneWidth";
import PaneDivider from "../../../shared/components/PaneDivider";
import { CLOUD_SYNC_ENABLED } from "../../../shared/config/edition";
import { getReleaseBuildInfo, type ReleaseBuildInfo } from "../../../shared/lib/releaseBuild";

interface SettingsPanelProps {
    t: (key: string) => string;
    theme: string;
    language: Locale;
    nativePalette: string;
    nativeCustomColor?: string;
    setNativePalette: (val: string) => void;
    colorMode: string;
    showAppBorder: boolean;
    setShowAppBorder: (val: boolean) => void;
    showSourceAppIcon: boolean;
    setShowSourceAppIcon: (val: boolean) => void;
    hideUnselectedItemActions: boolean;
    setHideUnselectedItemActions: (val: boolean) => void;
    clipboardItemFontSize: number;
    setClipboardItemFontSize: (val: number) => void;
    clipboardTagFontSize: number;
    setClipboardTagFontSize: (val: number) => void;

    // State
    collapsedGroups: Record<string, boolean>;
    settingsSubpage: SettingsSubpage;
    autoStart: boolean;
    elevatedAutostart: boolean;
    silentStart: boolean;
    persistent: boolean;
    persistentLimitEnabled: boolean;
    persistentLimit: number;
    deduplicate: boolean;
    captureFiles: boolean;
    captureRichText: boolean;
    richTextSnapshotPreview: boolean;
    deleteAfterPaste: boolean;
    moveToTopAfterPaste: boolean;
    clickCopyOnly: boolean;
    clickPasteMode: ClickPasteMode;
    pasteMethod: string;
    setPasteMethod: (val: string) => void;
    registryWinVEnabled: boolean;
    setRegistryWinVEnabled: (val: boolean) => void;
    sequentialMode: boolean;
    sequentialHotkey: string;
    isRecordingSequential: boolean;
    plainPasteHotkey: string;
    isRecordingPlain: boolean;
    searchHotkey: string;
    isRecordingSearch: boolean;
    quickPasteModifier: "disabled" | "ctrl" | "alt" | "shift" | "win";
    setQuickPasteModifier: (val: "disabled" | "ctrl" | "alt" | "shift" | "win") => void;
    privacyProtection: boolean;
    privacyProtectionKinds: string[];
    setPrivacyProtectionKinds: (val: string[]) => void;
    privacyProtectionCustomRules: string;
    setPrivacyProtectionCustomRules: (val: string) => void;
    sensitiveMaskPrefixVisible: number;
    setSensitiveMaskPrefixVisible: (val: number) => void;
    sensitiveMaskSuffixVisible: number;
    setSensitiveMaskSuffixVisible: (val: number) => void;
    sensitiveMaskEmailDomain: boolean;
    setSensitiveMaskEmailDomain: (val: boolean) => void;
    cleanupRules: string;
    setCleanupRules: (val: string) => void;
    appCleanupPolicies: AppCleanupPolicy[];
    setAppCleanupPolicies: (val: AppCleanupPolicy[]) => void;
    hotkey: string;
    showHotkeyHint: boolean;
    showSearchBox: boolean;
    setShowSearchBox: (val: boolean) => void;
    searchBoxPinned: boolean;
    setSearchBoxPinned: (val: boolean) => void;
    scrollTopButtonEnabled: boolean;
    setScrollTopButtonEnabled: (val: boolean) => void;
    emojiPanelEnabled: boolean;
    setEmojiPanelEnabled: (val: boolean) => void;
    tagManagerEnabled: boolean;
    setTagManagerEnabled: (val: boolean) => void;
    arrowKeySelection: boolean;
    setArrowKeySelection: (val: boolean) => void;
    lightModeEnabled: boolean;
    setLightModeEnabled: (val: boolean) => void;
    lightModeIdleMinutes: number;
    setLightModeIdleMinutes: (val: number) => void;


    soundEnabled: boolean;
    setSoundEnabled: (val: boolean) => void;
    pasteSoundEnabled: boolean;
    setPasteSoundEnabled: (val: boolean) => void;
    soundVolume: number;
    setSoundVolume: (val: number) => void;
    hideTrayIcon: boolean;
    setHideTrayIcon: (val: boolean) => void;
    hideDockIcon: boolean;
    setHideDockIcon: (val: boolean) => void;
    edgeDocking: boolean;
    setEdgeDocking: (val: boolean) => void;
    followMouse: boolean;
    setFollowMouse: (val: boolean) => void;
    customBackground: string;
    setCustomBackground: (val: string) => void;
    customBackgroundOpacity: number;
    setCustomBackgroundOpacity: (val: number) => void;
    surfaceOpacity: number;
    setSurfaceOpacity: (val: number) => void;


    mqttEnabled: boolean;
    mqttServer: string;
    mqttPort: string;
    mqttUser: string;
    mqttPass: string;
    mqttTopic: string;
    mqttProtocol: string;
    mqttWsPath: string;
    mqttNotificationEnabled: boolean;
    cloudSyncEnabled: boolean;
    cloudSyncAuto: boolean;
    cloudSyncProvider: "http" | "webdav";
    cloudSyncServer: string;
    cloudSyncApiKey: string;
    cloudSyncIntervalSec: string;
    cloudSyncSnapshotIntervalMin: string;
    cloudSyncWebdavUrl: string;
    cloudSyncWebdavUsername: string;
    cloudSyncWebdavPassword: string;
    cloudSyncWebdavBasePath: string;
    cloudSyncContentPrefs: CloudSyncContentPrefs;
    setCloudSyncContentPrefs: (val: CloudSyncContentPrefs) => void;

    fileServerEnabled: boolean;
    fileServerPort: string;
    localIp: string;
    availableIps?: string[];
    setLocalIp?: (val: string) => void;
    actualPort: string;
    fileTransferAccessToken: string;
    fileTransferLanKey: string;
    fileTransferAutoOpen: boolean;
    showAutoCloseHint: boolean;
    fileServerAutoClose: boolean;
    fileTransferAutoCopy: boolean;
    fileTransferPath: string;

    installedApps: InstalledAppOption[];
    appSettings: Record<string, string>;
    defaultApps: DefaultAppsMap;
    showAppSelector: string | null;
    dataPath: string;

    // Setters/Actions
    toggleGroup: (group: string) => void;
    setSettingsSubpage: (val: SettingsSubpage) => void;
    setAutoStart: (val: boolean) => void;
    setElevatedAutostart: (val: boolean) => void;
    setSilentStart: (val: boolean) => void;
    setPersistent: (val: boolean) => void;
    setPersistentLimitEnabled: (val: boolean) => void;
    setPersistentLimit: (val: number) => void;
    setDeduplicate: (val: boolean) => void;
    setCaptureFiles: (val: boolean) => void;
    setCaptureRichText: (val: boolean) => void;
    setRichTextSnapshotPreview: (val: boolean) => void;
    setDeleteAfterPaste: (val: boolean) => void;
    setMoveToTopAfterPaste: (val: boolean) => void;
    setClickCopyOnly: (val: boolean) => void;
    setClickPasteMode: (val: ClickPasteMode) => void;
    saveAppSetting: (key: string, val: string) => void;
    setSequentialModeState: (val: boolean) => void;
    setIsRecordingSequential: (val: boolean) => void;
    updateSequentialHotkey: (key: string) => void;
    setIsRecordingPlain: (val: boolean) => void;
    updatePlainPasteHotkey: (key: string) => void;
    setIsRecordingSearch: (val: boolean) => void;
    updateSearchHotkey: (key: string) => void;
    setPrivacyProtection: (val: boolean) => void;
    setShowHotkeyHint: (val: boolean) => void;
    setIsRecording: (val: boolean) => void;
    isRecording: boolean;
    hotkeyParts: string[];
    updateHotkey: (key: string) => void;

    setTheme: (val: string) => void;
    setColorMode: (val: string) => void;
    setLanguage: (val: Locale) => void;


    compactMode: boolean;
    setCompactMode: (val: boolean) => void;
    compactShowTime: boolean;
    setCompactShowTime: (val: boolean) => void;
    splitViewEnabled: boolean;
    setSplitViewEnabled: (val: boolean) => void;
    checkHotkeyConflict: (newHotkey: string, mode: 'main' | 'sequential' | 'plain' | 'search') => boolean;


    setMqttEnabled: (val: boolean) => void;
    saveMqtt: (key: string, val: string) => void;
    setMqttServer: (val: string) => void;
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
    saveCloudSync: (key: string, val: string) => void;

    setFileServerEnabled: (val: boolean) => void;
    setFileServerPort: (val: string) => void;
    setFileTransferAutoOpen: (val: boolean) => void;
    setShowAutoCloseHint: (val: boolean) => void;
    setFileServerAutoClose: (val: boolean) => void;
    setFileTransferAutoCopy: (val: boolean) => void;
    fetchEffectiveTransferPath: () => void;

    setShowAppSelector: (val: string | null) => void;
    handleResetSettings: () => void;
    onOpenChat?: () => void;

    // AI Settings
    aiEnabled: boolean;
    setAiEnabled: (val: boolean) => void;
    aiTargetLang: string;
    setAiTargetLang: (val: string) => void;
    saveSetting: (key: string, val: string) => void;
    aiProfiles: AiProfile[];
    setAiProfiles: (val: AiProfile[]) => void;
    aiAssignedProfileTask: string;
    setAiAssignedProfileTask: (id: string) => void;
    aiAssignedProfileMouthpiece: string;
    setAiAssignedProfileMouthpiece: (id: string) => void;
    aiAssignedProfileTranslate: string;
    setAiAssignedProfileTranslate: (id: string) => void;
}

const ABOUT_SECTION = "about";
const ADVANCED_ENTRY = "advanced";
const noop = () => {};

const SECTION_ICONS: Record<string, LucideIcon> = {
    general: Settings2,
    appearance: Palette,
    clipboard: Clipboard,
    [ADVANCED_ENTRY]: ListFilter,
    default_apps: AppWindow,
    ai: Sparkles,
    sync: MessageSquareText,
    cloud_sync: Cloud,
    file_transfer: Wifi,
    secure_transfer: ShieldCheck,
    data: Database
};

const NAV_GROUPS: { labelKey: string; ids: string[] }[] = [
    { labelKey: "settings_nav_group_basic", ids: ["general", "appearance"] },
    { labelKey: "settings_nav_group_clipboard", ids: ["clipboard", ADVANCED_ENTRY, "default_apps", "ai"] },
    { labelKey: "settings_nav_group_sync", ids: ["sync", "cloud_sync", "file_transfer"] },
    { labelKey: "settings_nav_group_data", ids: ["secure_transfer", "data"] }
];

const SettingsPanel = (props: SettingsPanelProps) => {
    const {
        t, theme, nativePalette, nativeCustomColor, setNativePalette, language, colorMode, showAppBorder, setShowAppBorder, showSourceAppIcon, setShowSourceAppIcon, hideUnselectedItemActions, setHideUnselectedItemActions,
        collapsedGroups, settingsSubpage, autoStart, elevatedAutostart, silentStart, persistent, persistentLimitEnabled, persistentLimit, deduplicate, captureFiles, captureRichText, richTextSnapshotPreview, deleteAfterPaste, moveToTopAfterPaste, clickCopyOnly, clickPasteMode, pasteMethod, setPasteMethod, registryWinVEnabled, setRegistryWinVEnabled,
        sequentialMode, sequentialHotkey, isRecordingSequential,
        plainPasteHotkey, isRecordingPlain, searchHotkey, isRecordingSearch, quickPasteModifier, setQuickPasteModifier,
        privacyProtection, privacyProtectionKinds, setPrivacyProtectionKinds, privacyProtectionCustomRules, setPrivacyProtectionCustomRules, sensitiveMaskPrefixVisible, setSensitiveMaskPrefixVisible, sensitiveMaskSuffixVisible, setSensitiveMaskSuffixVisible, sensitiveMaskEmailDomain, setSensitiveMaskEmailDomain, cleanupRules, setCleanupRules, appCleanupPolicies, setAppCleanupPolicies, setShowSearchBox, searchBoxPinned, setSearchBoxPinned, scrollTopButtonEnabled, setScrollTopButtonEnabled, arrowKeySelection, setArrowKeySelection,
        lightModeEnabled, setLightModeEnabled, lightModeIdleMinutes, setLightModeIdleMinutes,
        soundEnabled, setSoundEnabled, pasteSoundEnabled, setPasteSoundEnabled,
        soundVolume, setSoundVolume,
        hideTrayIcon, setHideTrayIcon,
        hideDockIcon, setHideDockIcon,
        edgeDocking, setEdgeDocking,
        followMouse, setFollowMouse,
        customBackground, setCustomBackground,
        customBackgroundOpacity, setCustomBackgroundOpacity,
        surfaceOpacity, setSurfaceOpacity,
        mqttEnabled, mqttServer, mqttPort, mqttUser, mqttPass, mqttTopic, mqttProtocol, mqttWsPath, mqttNotificationEnabled,
        cloudSyncEnabled, cloudSyncAuto, cloudSyncIntervalSec, cloudSyncSnapshotIntervalMin, cloudSyncWebdavUrl, cloudSyncWebdavUsername, cloudSyncWebdavPassword, cloudSyncWebdavBasePath, cloudSyncContentPrefs,
        fileServerEnabled, fileServerPort, localIp, availableIps, setLocalIp, actualPort, fileTransferAccessToken, fileTransferLanKey, fileTransferAutoOpen, showAutoCloseHint, fileServerAutoClose, fileTransferAutoCopy, fileTransferPath,
        installedApps, appSettings, defaultApps, showAppSelector, dataPath,

        toggleGroup, setSettingsSubpage, setAutoStart, setElevatedAutostart, setSilentStart, setPersistent, setPersistentLimitEnabled, setPersistentLimit, setDeduplicate, setCaptureFiles, setCaptureRichText, setRichTextSnapshotPreview, setDeleteAfterPaste, setMoveToTopAfterPaste, setClickCopyOnly, setClickPasteMode, saveAppSetting,
        setSequentialModeState, setIsRecordingSequential, updateSequentialHotkey,
        setIsRecordingPlain, updatePlainPasteHotkey,
        setIsRecordingSearch, updateSearchHotkey,
        setPrivacyProtection,
        setIsRecording, isRecording, hotkey, hotkeyParts, updateHotkey,
        setTheme, setColorMode, setLanguage, compactMode, setCompactMode, compactShowTime, setCompactShowTime, splitViewEnabled, setSplitViewEnabled, checkHotkeyConflict,
        clipboardItemFontSize, setClipboardItemFontSize, clipboardTagFontSize, setClipboardTagFontSize,
        emojiPanelEnabled, setEmojiPanelEnabled, tagManagerEnabled, setTagManagerEnabled,
        setMqttEnabled, saveMqtt, setMqttServer, setMqttPort, setMqttUser, setMqttPass, setMqttTopic, setMqttProtocol, setMqttWsPath, setMqttNotificationEnabled,
        setCloudSyncEnabled, setCloudSyncAuto, setCloudSyncIntervalSec, setCloudSyncSnapshotIntervalMin, setCloudSyncWebdavUrl, setCloudSyncWebdavUsername, setCloudSyncWebdavPassword, setCloudSyncWebdavBasePath, setCloudSyncContentPrefs, saveCloudSync,
        setFileServerEnabled, setFileServerPort, setFileTransferAutoOpen, setShowAutoCloseHint, setFileServerAutoClose, setFileTransferAutoCopy, fetchEffectiveTransferPath,
        setShowAppSelector, handleResetSettings,
        aiEnabled, setAiEnabled, aiTargetLang, setAiTargetLang, saveSetting,
        onOpenChat,
        aiProfiles, setAiProfiles, aiAssignedProfileTask, setAiAssignedProfileTask, aiAssignedProfileMouthpiece, setAiAssignedProfileMouthpiece, aiAssignedProfileTranslate, setAiAssignedProfileTranslate
    } = props;

    const [emailCopied, setEmailCopied] = useState(false);
    const [appVersion, setAppVersion] = useState("");
    const [releaseBuildInfo, setReleaseBuildInfo] = useState<ReleaseBuildInfo>({
        channel: "stable",
        portable: false
    });
    const [mqttStatus, setMqttStatus] = useState<"connected" | "disconnected" | "connecting">("disconnected");
    const [cloudSyncStatus, setCloudSyncStatus] = useState<CloudSyncStatusPayload>({
        state: "disabled",
        running: false,
        last_sync_at: null,
        last_error: null,
        uploaded_items: 0,
        received_items: 0
    });
    const [cloudSyncNowRunning, setCloudSyncNowRunning] = useState(false);
    const [editingProfile, setEditingProfile] = useState<EditableAiProfile | null>(null);
    const [profileStatuses, setProfileStatuses] = useState<AiProfileStatusMap>({});
    const [updateStatus, setUpdateStatus] = useState<string>("");
    // Removed updateModalData
    const [privacyKindsOpen, setPrivacyKindsOpen] = useState(false);
    const [privacyRulesOpen, setPrivacyRulesOpen] = useState(false);
    const isWide = useWideWindow();
    const wideLayout = splitViewEnabled && isWide;
    const [activeSection, setActiveSection] = useState("general");
    const { flaggedSections, acknowledge: acknowledgeUnreadable } = useUnreadableSecrets();
    const attentionBadge = (
        <span className="settings-attention-badge" role="img" aria-label={t("secret_reenter_hint")} title={t("secret_reenter_hint")}>!</span>
    );
    const wideRef = useRef<HTMLDivElement | null>(null);
    const navRef = useRef<HTMLElement | null>(null);
    const { value: navWidth, resizing: navResizing, dividerProps: navDividerProps } = usePaneWidth({
        storageKey: "app.pane.settings",
        containerRef: wideRef,
        paneRef: navRef,
        min: 148,
        max: 320,
        minRest: 360
    });

    const applyFileServerPort = async (portStr: string) => {
        const port = Number(portStr);
        if (!Number.isInteger(port) || port < 1 || port > 65535) return;
        if (!fileServerEnabled) return;
        try {
            await invoke("toggle_file_server", { enabled: false });
            await invoke("toggle_file_server", { enabled: true, port });
        } catch (e) {
            console.error(e);
        }
    };

    const checkModelStatus = async (profile: AiProfile) => {
        setProfileStatuses(prev => ({ ...prev, [profile.id]: 'loading' }));
        try {
            const result = await invoke<string>("check_ai_connectivity", {
                baseUrl: profile.baseUrl,
                apiKey: profile.apiKey,
                model: profile.model,
                provider: profile.provider ?? null
            });
            if (result === "success") {
                setProfileStatuses(prev => ({ ...prev, [profile.id]: 'success' }));
            }
        } catch (e: unknown) {
            console.error("AI Check failed:", e);
            setProfileStatuses(prev => ({ ...prev, [profile.id]: 'error' }));
            // The red dot alone left users guessing (#114).
            void emit("toast", `${t("ai_check_failed")}${formatError(e, t)}`);
        }
    };

    const handleCloudSyncNow = async () => {
        if (!CLOUD_SYNC_ENABLED) return;
        setCloudSyncNowRunning(true);
        try {
            const status = await invoke<CloudSyncStatusPayload>("cloud_sync_now");
            setCloudSyncStatus(status);
        } catch (err) {
            console.error("Cloud sync now failed:", err);
            void emit("toast", failureText(t("sync_now_failed"), syncErrorText(err, t)));
        } finally {
            setCloudSyncNowRunning(false);
        }
    };

    const aiTaskAssignments = [
        { value: aiAssignedProfileTask, set: setAiAssignedProfileTask, key: 'ai_assigned_profile_task' },
        { value: aiAssignedProfileMouthpiece, set: setAiAssignedProfileMouthpiece, key: 'ai_assigned_profile_mouthpiece' },
        { value: aiAssignedProfileTranslate, set: setAiAssignedProfileTranslate, key: 'ai_assigned_profile_translate' },
    ];

    /** Saves the model list and moves tasks left with nothing to run on. */
    const commitAiProfiles = (newProfiles: AiProfile[], deletedId?: string) => {
        setAiProfiles(newProfiles);
        saveSetting('ai_profiles', JSON.stringify(newProfiles));
        for (const task of aiTaskAssignments) {
            let next = resolveTaskProfile(task.value, newProfiles);
            if (next === deletedId) next = 'default';
            if (next !== task.value) {
                task.set(next);
                saveSetting(task.key, next);
            }
        }
    };

    const handleSaveProfile = (profile: EditableAiProfile) => {
        let newProfiles: AiProfile[];
        if (profile.isNew) {
            const { isNew, id: _id, ...rest } = profile;
            newProfiles = [...aiProfiles, { ...rest, id: Date.now().toString() }];
        } else {
            if (!profile.id) return;
            const { isNew, ...rest } = profile;
            const updatedProfile: AiProfile = { ...rest, id: profile.id };
            newProfiles = aiProfiles.map(p => p.id === profile.id ? updatedProfile : p);
        }
        commitAiProfiles(newProfiles);
        setEditingProfile(null);
    };

    const handleDeleteProfile = (id: string) => {
        commitAiProfiles(aiProfiles.filter(p => p.id !== id), id);
    };

    useEffect(() => {
        getVersion()
            .then(v => setAppVersion(v))
            .catch(err => {
                console.error("Failed to get version:", err);
                setAppVersion("0.2.0");
            });
        getReleaseBuildInfo().then(setReleaseBuildInfo);

        const unlistenMqtt = listen<string>("mqtt-status", (event) => {
            console.log('[MQTT STATUS] Received status:', event.payload);
            setMqttStatus(event.payload as "connected" | "disconnected" | "connecting");
        });

        let unlistenCloud: Promise<() => void> | null = null;
        if (CLOUD_SYNC_ENABLED) {
            unlistenCloud = listen<CloudSyncStatusPayload>("cloud-sync-status", (event) => {
                setCloudSyncStatus(event.payload);
            });
            invoke<CloudSyncStatusPayload>("get_cloud_sync_status")
                .then(setCloudSyncStatus)
                .catch(console.error);
        }

        Promise.all([
            invoke<boolean>("get_mqtt_status"),
            invoke<boolean>("get_mqtt_running")
        ]).then(([connected, running]) => {
            console.log('[MQTT INIT] connected:', connected, 'running:', running);
            if (connected) {
                setMqttStatus("connected");
            } else if (running) {
                setMqttStatus("connecting");
            } else {
                console.log('[MQTT INIT] Keeping default disconnected state');
            }
        }).catch(console.error);

        return () => {
            unlistenMqtt.then(f => f());
            if (unlistenCloud) {
                unlistenCloud.then(f => f());
            }
        };
    }, []);

    const openAdvancedSettingsWindow = useCallback(() => {
        setSettingsSubpage("advanced");
    }, [setSettingsSubpage]);

    // Narrow windows show an accordion of collapsible groups. Wide ones list the
    // sections on the left and show the selected group, always open, on the right.
    const { key: e2eKey, save: saveE2eKey } = useE2eKey();
    const groupState = (id: string) => wideLayout
        ? { collapsed: false, onToggle: noop }
        : { collapsed: collapsedGroups[id], onToggle: () => toggleGroup(id) };

    const sections: { id: string; label: string; node: ReactNode }[] = [
        {
            id: "general",
            label: t("general_settings"),
            node: (
                <GeneralSettingsGroup
                    t={t}
                    {...groupState('general')}
                    LabelWithHint={LabelWithHint}
                    autoStart={autoStart}
                    setAutoStart={setAutoStart}
                    elevatedAutostart={elevatedAutostart}
                    setElevatedAutostart={setElevatedAutostart}
                    silentStart={silentStart}
                    setSilentStart={setSilentStart}
                    hideTrayIcon={hideTrayIcon}
                    setHideTrayIcon={setHideTrayIcon}
                    hideDockIcon={hideDockIcon}
                    setHideDockIcon={setHideDockIcon}
                    edgeDocking={edgeDocking}
                    setEdgeDocking={setEdgeDocking}
                    followMouse={followMouse}
                    setFollowMouse={setFollowMouse}
                    soundEnabled={soundEnabled}
                    setSoundEnabled={setSoundEnabled}
                    pasteSoundEnabled={pasteSoundEnabled}
                    setPasteSoundEnabled={setPasteSoundEnabled}
                    soundVolume={soundVolume}
                    setSoundVolume={setSoundVolume}
                    searchBoxPinned={searchBoxPinned}
                    setSearchBoxPinned={setSearchBoxPinned}
                    setShowSearchBox={setShowSearchBox}
                    scrollTopButtonEnabled={scrollTopButtonEnabled}
                    setScrollTopButtonEnabled={setScrollTopButtonEnabled}
                    emojiPanelEnabled={emojiPanelEnabled}
                    setEmojiPanelEnabled={setEmojiPanelEnabled}
                    tagManagerEnabled={tagManagerEnabled}
                    setTagManagerEnabled={setTagManagerEnabled}
                    arrowKeySelection={arrowKeySelection}
                    setArrowKeySelection={setArrowKeySelection}
                    lightModeEnabled={lightModeEnabled}
                    setLightModeEnabled={setLightModeEnabled}
                    lightModeIdleMinutes={lightModeIdleMinutes}
                    setLightModeIdleMinutes={setLightModeIdleMinutes}
                    saveAppSetting={saveAppSetting}
                />
            )
        },
        {
            id: "clipboard",
            label: t("clipboard_settings"),
            node: (
                <ClipboardSettingsGroup
                    t={t}
                    {...groupState('clipboard')}
                    LabelWithHint={LabelWithHint}
                    persistent={persistent}
                    setPersistent={setPersistent}
                    persistentLimitEnabled={persistentLimitEnabled}
                    setPersistentLimitEnabled={setPersistentLimitEnabled}
                    persistentLimit={persistentLimit}
                    setPersistentLimit={setPersistentLimit}
                    saveAppSetting={saveAppSetting}
                    deduplicate={deduplicate}
                    setDeduplicate={setDeduplicate}
                    captureFiles={captureFiles}
                    setCaptureFiles={setCaptureFiles}
                    captureRichText={captureRichText}
                    setCaptureRichText={setCaptureRichText}
                    richTextSnapshotPreview={richTextSnapshotPreview}
                    setRichTextSnapshotPreview={setRichTextSnapshotPreview}
                    plainPasteHotkey={plainPasteHotkey}
                    isRecordingPlain={isRecordingPlain}
                    setIsRecordingPlain={setIsRecordingPlain}
                    updatePlainPasteHotkey={updatePlainPasteHotkey}
                    searchHotkey={searchHotkey}
                    isRecordingSearch={isRecordingSearch}
                    setIsRecordingSearch={setIsRecordingSearch}
                    updateSearchHotkey={updateSearchHotkey}
                    quickPasteModifier={quickPasteModifier}
                    setQuickPasteModifier={setQuickPasteModifier}
                    deleteAfterPaste={deleteAfterPaste}
                    setDeleteAfterPaste={setDeleteAfterPaste}
                    moveToTopAfterPaste={moveToTopAfterPaste}
                    clickCopyOnly={clickCopyOnly}
                    clickPasteMode={clickPasteMode}
                    setMoveToTopAfterPaste={setMoveToTopAfterPaste}
                    setClickCopyOnly={setClickCopyOnly}
                    setClickPasteMode={setClickPasteMode}
                    pasteMethod={pasteMethod}
                    setPasteMethod={setPasteMethod}
                    registryWinVEnabled={registryWinVEnabled}
                    setRegistryWinVEnabled={setRegistryWinVEnabled}
                    sequentialMode={sequentialMode}
                    setSequentialModeState={setSequentialModeState}
                    sequentialHotkey={sequentialHotkey}
                    isRecordingSequential={isRecordingSequential}
                    setIsRecordingSequential={setIsRecordingSequential}
                    updateSequentialHotkey={updateSequentialHotkey}
                    checkHotkeyConflict={checkHotkeyConflict}
                    privacyProtection={privacyProtection}
                    setPrivacyProtection={setPrivacyProtection}
                    privacyProtectionKinds={privacyProtectionKinds}
                    setPrivacyProtectionKinds={setPrivacyProtectionKinds}
                    privacyProtectionCustomRules={privacyProtectionCustomRules}
                    setPrivacyProtectionCustomRules={setPrivacyProtectionCustomRules}
                    sensitiveMaskPrefixVisible={sensitiveMaskPrefixVisible}
                    setSensitiveMaskPrefixVisible={setSensitiveMaskPrefixVisible}
                    sensitiveMaskSuffixVisible={sensitiveMaskSuffixVisible}
                    setSensitiveMaskSuffixVisible={setSensitiveMaskSuffixVisible}
                    sensitiveMaskEmailDomain={sensitiveMaskEmailDomain}
                    setSensitiveMaskEmailDomain={setSensitiveMaskEmailDomain}
                    privacyKindsOpen={privacyKindsOpen}
                    setPrivacyKindsOpen={setPrivacyKindsOpen}
                    privacyRulesOpen={privacyRulesOpen}
                    setPrivacyRulesOpen={setPrivacyRulesOpen}

                    isRecording={isRecording}
                    setIsRecording={setIsRecording}
                    hotkeyParts={hotkeyParts}
                    updateHotkey={updateHotkey}
                    hotkey={hotkey}
                    appSettings={appSettings}
                    theme={theme}
                    colorMode={colorMode}
                />
            )
        },
        {
            id: "appearance",
            label: t("appearance_settings"),
            node: (
                <AppearanceSettingsGroup
                    t={t}
                    {...groupState('appearance')}
                    LabelWithHint={LabelWithHint}
                    theme={theme}
                    setTheme={setTheme}
                    nativePalette={nativePalette}
                    nativeCustomColor={nativeCustomColor}
                    setNativePalette={setNativePalette}
                    colorMode={colorMode}
                    setColorMode={setColorMode}
                    language={language}
                    setLanguage={setLanguage}
                    showAppBorder={showAppBorder}
                    setShowAppBorder={setShowAppBorder}
                    showSourceAppIcon={showSourceAppIcon}
                    setShowSourceAppIcon={setShowSourceAppIcon}
                    hideUnselectedItemActions={hideUnselectedItemActions}
                    setHideUnselectedItemActions={setHideUnselectedItemActions}
                    compactMode={compactMode}
                    setCompactMode={setCompactMode}
                    compactShowTime={compactShowTime}
                    setCompactShowTime={setCompactShowTime}
                    splitViewEnabled={splitViewEnabled}
                    setSplitViewEnabled={setSplitViewEnabled}
                    clipboardItemFontSize={clipboardItemFontSize}
                    setClipboardItemFontSize={setClipboardItemFontSize}
                    clipboardTagFontSize={clipboardTagFontSize}
                    setClipboardTagFontSize={setClipboardTagFontSize}
                    customBackground={customBackground}
                    setCustomBackground={setCustomBackground}
                    customBackgroundOpacity={customBackgroundOpacity}
                    setCustomBackgroundOpacity={setCustomBackgroundOpacity}
                    surfaceOpacity={surfaceOpacity}
                    setSurfaceOpacity={setSurfaceOpacity}
                    saveAppSetting={saveAppSetting}
                />
            )
        },
        {
            id: "sync",
            label: t("sync_settings"),
            node: (
                <SyncSettingsGroup
                    t={t}
                    {...groupState('sync')}
                    LabelWithHint={LabelWithHint}
                    mqttEnabled={mqttEnabled}
                    mqttStatus={mqttStatus}
                    setMqttEnabled={setMqttEnabled}
                    saveMqtt={saveMqtt}
                    mqttProtocol={mqttProtocol}
                    setMqttProtocol={setMqttProtocol}
                    mqttWsPath={mqttWsPath}
                    setMqttWsPath={setMqttWsPath}
                    mqttServer={mqttServer}
                    setMqttServer={setMqttServer}
                    mqttPort={mqttPort}
                    setMqttPort={setMqttPort}
                    mqttUser={mqttUser}
                    setMqttUser={setMqttUser}
                    mqttPass={mqttPass}
                    setMqttPass={setMqttPass}
                    mqttTopic={mqttTopic}
                    setMqttTopic={setMqttTopic}
                    mqttNotificationEnabled={mqttNotificationEnabled}
                    setMqttNotificationEnabled={setMqttNotificationEnabled}
                    e2eKey={e2eKey}
                    saveE2eKey={saveE2eKey}
                />
            )
        },
        ...(CLOUD_SYNC_ENABLED
            ? [{
                id: "cloud_sync",
                label: t("cloud_sync_settings"),
                node: (
                    <CloudSyncSettingsGroup
                        t={t}
                        {...groupState('cloud_sync')}
                        LabelWithHint={LabelWithHint}
                        cloudSyncEnabled={cloudSyncEnabled}
                        setCloudSyncEnabled={setCloudSyncEnabled}
                        cloudSyncAuto={cloudSyncAuto}
                        setCloudSyncAuto={setCloudSyncAuto}
                        cloudSyncIntervalSec={cloudSyncIntervalSec}
                        setCloudSyncIntervalSec={setCloudSyncIntervalSec}
                        cloudSyncSnapshotIntervalMin={cloudSyncSnapshotIntervalMin}
                        setCloudSyncSnapshotIntervalMin={setCloudSyncSnapshotIntervalMin}
                        cloudSyncWebdavUrl={cloudSyncWebdavUrl}
                        setCloudSyncWebdavUrl={setCloudSyncWebdavUrl}
                        cloudSyncWebdavUsername={cloudSyncWebdavUsername}
                        setCloudSyncWebdavUsername={setCloudSyncWebdavUsername}
                        cloudSyncWebdavPassword={cloudSyncWebdavPassword}
                        setCloudSyncWebdavPassword={setCloudSyncWebdavPassword}
                        cloudSyncWebdavBasePath={cloudSyncWebdavBasePath}
                        setCloudSyncWebdavBasePath={setCloudSyncWebdavBasePath}
                        cloudSyncContentPrefs={cloudSyncContentPrefs}
                        setCloudSyncContentPrefs={setCloudSyncContentPrefs}
                        saveCloudSync={saveCloudSync}
                        status={cloudSyncStatus}
                        syncingNow={cloudSyncNowRunning}
                        onSyncNow={handleCloudSyncNow}
                        e2eKey={e2eKey}
                        saveE2eKey={saveE2eKey}
                    />
                )
            }]
            : []),
        {
            id: "ai",
            label: t("ai_settings"),
            node: (
                <AiSettingsGroup
                    t={t}
                    {...groupState('ai')}
                    aiEnabled={aiEnabled}
                    setAiEnabled={setAiEnabled}
                    saveSetting={saveSetting}
                    aiProfiles={aiProfiles}
                    profileStatuses={profileStatuses}
                    checkModelStatus={checkModelStatus}
                    setEditingProfile={setEditingProfile}
                    handleDeleteProfile={handleDeleteProfile}
                    aiAssignedProfileTask={aiAssignedProfileTask}
                    setAiAssignedProfileTask={setAiAssignedProfileTask}
                    aiAssignedProfileMouthpiece={aiAssignedProfileMouthpiece}
                    setAiAssignedProfileMouthpiece={setAiAssignedProfileMouthpiece}
                    aiAssignedProfileTranslate={aiAssignedProfileTranslate}
                    setAiAssignedProfileTranslate={setAiAssignedProfileTranslate}
                    aiTargetLang={aiTargetLang}
                    setAiTargetLang={setAiTargetLang}
                    theme={theme}
                />
            )
        },
        {
            id: "file_transfer",
            label: t("file_transfer"),
            node: (
                <FileTransferSettingsGroup
                    t={t}
                    {...groupState('file_transfer')}
                    fileServerEnabled={fileServerEnabled}
                    setFileServerEnabled={setFileServerEnabled}
                    fileServerPort={fileServerPort}
                    setFileServerPort={setFileServerPort}
                    applyFileServerPort={applyFileServerPort}
                    localIp={localIp}
                    availableIps={availableIps}
                    setLocalIp={setLocalIp}
                    actualPort={actualPort}
                    accessToken={fileTransferAccessToken}
                    lanKey={fileTransferLanKey}
                    fileTransferAutoOpen={fileTransferAutoOpen}
                    setFileTransferAutoOpen={setFileTransferAutoOpen}
                    showAutoCloseHint={showAutoCloseHint}
                    setShowAutoCloseHint={setShowAutoCloseHint}
                    fileServerAutoClose={fileServerAutoClose}
                    setFileServerAutoClose={setFileServerAutoClose}
                    fileTransferAutoCopy={fileTransferAutoCopy}
                    setFileTransferAutoCopy={setFileTransferAutoCopy}
                    onOpenChat={onOpenChat}
                    fileTransferPath={fileTransferPath}
                    saveSetting={saveSetting}
                    fetchEffectiveTransferPath={fetchEffectiveTransferPath}
                />
            )
        },
        {
            id: "default_apps",
            label: t("default_apps"),
            node: (
                <DefaultAppsSettingsGroup
                    t={t}
                    {...groupState('default_apps')}
                    installedApps={installedApps}
                    appSettings={appSettings}
                    defaultApps={defaultApps}
                    setShowAppSelector={setShowAppSelector}
                />
            )
        },
        {
            id: "secure_transfer",
            label: t("secure_transfer"),
            node: (
                <SecureTransfer t={t} {...groupState('secure_transfer')} />
            )
        },
        {
            id: "data",
            label: t("data_management"),
            node: (
                <DataSettingsGroup
                    t={t}
                    theme={theme}
                    {...groupState('data')}
                    dataPath={dataPath}
                    portable={releaseBuildInfo.portable}
                />
            )
        }
    ];

    // Keep the reader's place when the layout flips (window resized, or the
    // two-column toggle itself switched off from the appearance section).
    const sectionIds = sections.map((section) => section.id).join(" ");
    const wasWideRef = useRef(wideLayout);
    useEffect(() => {
        if (wasWideRef.current === wideLayout) return;
        wasWideRef.current = wideLayout;
        if (wideLayout) {
            const open = sectionIds.split(" ").find((id) => !collapsedGroups[id]);
            if (open) setActiveSection(open);
            return;
        }
        if (activeSection === ABOUT_SECTION || !collapsedGroups[activeSection]) return;
        toggleGroup(activeSection);
        requestAnimationFrame(() => {
            document
                .querySelector(`[data-settings-section="${activeSection}"] > .settings-group`)
                ?.scrollIntoView({ block: "start" });
        });
    }, [wideLayout, sectionIds, collapsedGroups, activeSection, toggleGroup]);

    const footer = (
        <SettingsFooter
            expanded={wideLayout}
            t={t}
            appVersion={appVersion}
            releaseChannel={releaseBuildInfo.channel}
            portable={releaseBuildInfo.portable}
            updateStatus={updateStatus}
            setUpdateStatus={setUpdateStatus}
            onResetSettings={handleResetSettings}
            emailCopied={emailCopied}
            setEmailCopied={setEmailCopied}
        />
    );

    const modals = (
        <>
            <AiProfileModal
                editingProfile={editingProfile}
                t={t}
                onClose={() => setEditingProfile(null)}
                onSave={handleSaveProfile}
                setEditingProfile={setEditingProfile}
            />

            <AppSelectorModal
                show={showAppSelector}
                installedApps={installedApps}
                theme={theme}
                colorMode={colorMode}
                t={t}
                onClose={() => setShowAppSelector(null)}
                onSave={saveAppSetting}
                resetLabel={showAppSelector && chosenApp(appSettings, showAppSelector)
                    ? t(unchosenFallback(appSettings, showAppSelector) === "text" ? "same_as_text_app" : "system_default")
                    : undefined}
            />
        </>
    );

    const renderNavItem = (id: string, label: string, onClick: () => void, trailing?: ReactNode) => {
        const Icon = SECTION_ICONS[id] ?? Info;
        const active = activeSection === id;
        return (
            <button
                key={id}
                type="button"
                className={`settings-nav-item${active ? " active" : ""}`}
                data-section={id}
                aria-current={active ? "page" : undefined}
                onClick={onClick}
            >
                <Icon className="settings-nav-icon" size={18} strokeWidth={1.75} aria-hidden="true" />
                <span className="settings-nav-label">{label}</span>
                {trailing}
            </button>
        );
    };

    const renderWide = () => {
        const current = sections.find((section) => section.id === activeSection);
        return (
            <div
                ref={wideRef}
                className="settings-wide"
                style={navWidth != null ? { ["--settings-nav-width" as string]: `${navWidth}px` } : undefined}
            >
                <nav ref={navRef} className="settings-nav" aria-label={t("settings")}>
                    {NAV_GROUPS.map((group) => {
                        const items = group.ids.flatMap((id) => {
                            if (id === ADVANCED_ENTRY) {
                                return [renderNavItem(
                                    id,
                                    t("advanced_settings"),
                                    openAdvancedSettingsWindow,
                                    <ChevronRight className="settings-nav-trailing" size={14} aria-hidden="true" />
                                )];
                            }
                            const section = sections.find((item) => item.id === id);
                            return section ? [renderNavItem(
                                section.id,
                                section.label,
                                () => {
                                    setActiveSection(section.id);
                                    acknowledgeUnreadable(section.id);
                                },
                                flaggedSections.has(section.id) ? attentionBadge : undefined
                            )] : [];
                        });
                        if (items.length === 0) return null;
                        return (
                            <div key={group.labelKey} className="settings-nav-group" role="group" aria-label={t(group.labelKey)}>
                                <div className="settings-nav-group-label" aria-hidden="true">{t(group.labelKey)}</div>
                                {items}
                            </div>
                        );
                    })}
                    <div className="settings-nav-footer">
                        {renderNavItem(ABOUT_SECTION, t("settings_about"), () => setActiveSection(ABOUT_SECTION))}
                    </div>
                </nav>
                <PaneDivider active={navResizing} label={t("pane_divider_hint")} {...navDividerProps} />
                {/* Keyed so switching sections starts at the top. */}
                <div className="settings-wide-content" key={activeSection}>
                    <div className="settings-wide-inner">
                        {activeSection === ABOUT_SECTION ? footer : (current ?? sections[0]).node}
                    </div>
                </div>
            </div>
        );
    };

    return (
        <SettingsHintProvider>
            <div className="settings-panel-root">
                {settingsSubpage === "advanced" ? (
                    <AdvancedSettingsGroup
                        t={t}
                        cleanupRules={cleanupRules}
                        setCleanupRules={setCleanupRules}
                        appCleanupPolicies={appCleanupPolicies}
                        setAppCleanupPolicies={setAppCleanupPolicies}
                        installedApps={installedApps}
                    />
                ) : wideLayout ? (
                    renderWide()
                ) : (
                    <>
                        {/* The wide layout's sidebar groups, as an inset list:
                            a flat run of a dozen equal cards was hard to scan. */}
                        {NAV_GROUPS.map((group) => {
                            const items = group.ids.flatMap((id) => {
                                const Icon = SECTION_ICONS[id] ?? Info;
                                const icon = <Icon className="settings-section-icon" size={16} strokeWidth={1.75} aria-hidden="true" />;
                                if (id === ADVANCED_ENTRY) {
                                    return [
                                        <div key={id} className="settings-section">
                                            {icon}
                                            <div className="settings-group collapsed">
                                                <button
                                                    type="button"
                                                    className="group-header settings-nav-card"
                                                    onClick={openAdvancedSettingsWindow}
                                                >
                                                    <h3>{t("advanced_settings")}</h3>
                                                    <ChevronRight size={16} />
                                                </button>
                                            </div>
                                        </div>
                                    ];
                                }
                                const section = sections.find((item) => item.id === id);
                                return section
                                    ? [
                                        <div
                                            key={section.id}
                                            className="settings-section"
                                            data-settings-section={section.id}
                                            onClickCapture={flaggedSections.has(section.id) ? () => acknowledgeUnreadable(section.id) : undefined}
                                        >
                                            {icon}
                                            {section.node}
                                            {flaggedSections.has(section.id) && attentionBadge}
                                        </div>
                                    ]
                                    : [];
                            });
                            if (items.length === 0) return null;
                            return (
                                <section key={group.labelKey} className="settings-narrow-group" aria-label={t(group.labelKey)}>
                                    <div className="settings-narrow-group-label" aria-hidden="true">{t(group.labelKey)}</div>
                                    <div className="settings-narrow-group-items">{items}</div>
                                </section>
                            );
                        })}

                        {footer}
                    </>
                )}
                {modals}
            </div>
        </SettingsHintProvider>
    );
};

export default memo(SettingsPanel);
