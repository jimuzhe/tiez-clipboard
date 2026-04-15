
import { memo, useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { ChevronRight, HelpCircle } from "lucide-react";
import { motion } from "framer-motion";
import type { Locale } from "../../../shared/types";
import type { DefaultAppsMap, InstalledAppOption, SettingsSubpage } from "../../app/types";
import type { AiProfile, AiProfileStatusMap, AppCleanupPolicy, EditableAiProfile } from "../types";
import AppSelectorModal from "./AppSelectorModal";


import AiProfileModal from "./AiProfileModal";
import GeneralSettingsGroup from "./groups/GeneralSettingsGroup";
import ClipboardSettingsGroup from "./groups/ClipboardSettingsGroup";
import AppearanceSettingsGroup from "./groups/AppearanceSettingsGroup";
import SyncSettingsGroup from "./groups/SyncSettingsGroup";
import CloudSyncSettingsGroup, { type CloudSyncStatusPayload } from "./groups/CloudSyncSettingsGroup";
import DefaultAppsSettingsGroup from "./groups/DefaultAppsSettingsGroup";
import DataSettingsGroup from "./groups/DataSettingsGroup";
import FileTransferSettingsGroup from "./groups/FileTransferSettingsGroup";
import AiSettingsGroup from "./groups/AiSettingsGroup";
import AdvancedSettingsGroup from "./groups/AdvancedSettingsGroup";
import SettingsFooter from "./SettingsFooter";
import { CLOUD_SYNC_ENABLED } from "../../../shared/config/edition";
import type { QuickPasteModifier } from "../../app/types";

const SETTINGS_DEBUG = import.meta.env.DEV;
const settingsLog = (...args: unknown[]) => {
    if (!SETTINGS_DEBUG) return;
    console.log(...args);
};

interface SettingsPanelProps {
    t: (key: string) => string;
    theme: string;
    language: Locale;
    colorMode: string;
    clipboardItemFontSize: number;
    setClipboardItemFontSize: (val: number) => void;
    clipboardTagFontSize: number;
    setClipboardTagFontSize: (val: number) => void;

    // State
    collapsedGroups: Record<string, boolean>;
    settingsSubpage: SettingsSubpage;
    setSettingsSubpage: (val: SettingsSubpage) => void;
    autoStart: boolean;
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
    sequentialMode: boolean;
    sequentialHotkey: string;
    isRecordingSequential: boolean;
    richPasteHotkey: string;
    isRecordingRich: boolean;
    searchHotkey: string;
    isRecordingSearch: boolean;
    quickPasteModifier: QuickPasteModifier;
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
    winClipboardDisabled: boolean;
    registryWinVEnabled: boolean;
    setRegistryWinVEnabled: (val: boolean) => void;
    showSearchBox: boolean;
    setShowSearchBox: (val: boolean) => void;
    scrollTopButtonEnabled: boolean;
    setScrollTopButtonEnabled: (val: boolean) => void;
    emojiPanelEnabled: boolean;
    setEmojiPanelEnabled: (val: boolean) => void;
    tagManagerEnabled: boolean;
    setTagManagerEnabled: (val: boolean) => void;
    arrowKeySelection: boolean;
    setArrowKeySelection: (val: boolean) => void;


    soundEnabled: boolean;
    setSoundEnabled: (val: boolean) => void;
    soundVolume: number;
    setSoundVolume: (val: number) => void;
    pasteSoundEnabled: boolean;
    setPasteSoundEnabled: (val: boolean) => void;
    pasteMethod: string;
    setPasteMethod: (val: string) => void;
    hideTrayIcon: boolean;
    setHideTrayIcon: (val: boolean) => void;
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

    fileServerEnabled: boolean;
    fileServerPort: string;
    localIp: string;
    availableIps?: string[];
    setLocalIp?: (val: string) => void;
    actualPort: string;
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
    setAutoStart: (val: boolean) => void;
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
    saveAppSetting: (key: string, val: string) => void;
    setSequentialModeState: (val: boolean) => void;
    setIsRecordingSequential: (val: boolean) => void;
    updateSequentialHotkey: (key: string) => void;
    setIsRecordingRich: (val: boolean) => void;
    updateRichPasteHotkey: (key: string) => void;
    setIsRecordingSearch: (val: boolean) => void;
    updateSearchHotkey: (key: string) => void;
    setQuickPasteModifier: (val: QuickPasteModifier) => void;
    setPrivacyProtection: (val: boolean) => void;
    setShowHotkeyHint: (val: boolean) => void;
    setIsRecording: (val: boolean) => void;
    isRecording: boolean;
    hotkeyParts: string[];
    updateHotkey: (key: string) => void;
    setWinClipboardDisabled: (val: boolean) => void;

    setTheme: (val: string) => void;
    setColorMode: (val: string) => void;
    setLanguage: (val: Locale) => void;
    showAppBorder: boolean;
    setShowAppBorder: (val: boolean) => void;
    showSourceAppIcon: boolean;
    setShowSourceAppIcon: (val: boolean) => void;

    compactMode: boolean;
    setCompactMode: (val: boolean) => void;
    checkHotkeyConflict: (newHotkey: string, mode: 'main' | 'sequential' | 'rich' | 'search') => boolean;


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
    aiThinkingBudget: string;
    setAiThinkingBudget: (val: string) => void;
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

const SettingsPanel = (props: SettingsPanelProps) => {
    const {
        t, theme, language, colorMode,
        collapsedGroups, settingsSubpage, setSettingsSubpage, autoStart, silentStart, persistent, persistentLimitEnabled, persistentLimit, deduplicate, captureFiles, captureRichText, richTextSnapshotPreview, deleteAfterPaste, moveToTopAfterPaste,
        sequentialMode, sequentialHotkey, isRecordingSequential,
        richPasteHotkey, isRecordingRich, searchHotkey, isRecordingSearch, quickPasteModifier,
        privacyProtection, privacyProtectionKinds, setPrivacyProtectionKinds, privacyProtectionCustomRules, setPrivacyProtectionCustomRules,
        sensitiveMaskPrefixVisible, setSensitiveMaskPrefixVisible, sensitiveMaskSuffixVisible, setSensitiveMaskSuffixVisible, sensitiveMaskEmailDomain, setSensitiveMaskEmailDomain,
        cleanupRules, setCleanupRules, appCleanupPolicies, setAppCleanupPolicies,
        registryWinVEnabled, setRegistryWinVEnabled, showSearchBox, setShowSearchBox, scrollTopButtonEnabled, setScrollTopButtonEnabled, arrowKeySelection, setArrowKeySelection,
        soundEnabled, setSoundEnabled, pasteSoundEnabled, setPasteSoundEnabled,
        soundVolume, setSoundVolume,
        pasteMethod, setPasteMethod,
        hideTrayIcon, setHideTrayIcon,
        edgeDocking, setEdgeDocking,
        followMouse, setFollowMouse,
        customBackground, setCustomBackground,
        customBackgroundOpacity, setCustomBackgroundOpacity,
        surfaceOpacity, setSurfaceOpacity,
        mqttEnabled, mqttServer, mqttPort, mqttUser, mqttPass, mqttTopic, mqttProtocol, mqttWsPath, mqttNotificationEnabled,
        cloudSyncEnabled, cloudSyncAuto, cloudSyncIntervalSec, cloudSyncSnapshotIntervalMin, cloudSyncWebdavUrl, cloudSyncWebdavUsername, cloudSyncWebdavPassword, cloudSyncWebdavBasePath,
        fileServerEnabled, fileServerPort, localIp, availableIps, setLocalIp, actualPort, fileTransferAutoOpen, showAutoCloseHint, fileServerAutoClose, fileTransferAutoCopy, fileTransferPath,
        installedApps, appSettings, defaultApps, showAppSelector, dataPath,

        toggleGroup, setAutoStart, setSilentStart, setPersistent, setPersistentLimitEnabled, setPersistentLimit, setDeduplicate, setCaptureFiles, setCaptureRichText, setRichTextSnapshotPreview, setDeleteAfterPaste, setMoveToTopAfterPaste, saveAppSetting,
        setSequentialModeState, setIsRecordingSequential, updateSequentialHotkey,
        setIsRecordingRich, updateRichPasteHotkey,
        setIsRecordingSearch, updateSearchHotkey, setQuickPasteModifier,
        setPrivacyProtection,
        setIsRecording, isRecording, hotkey, hotkeyParts, updateHotkey,
        setTheme, setColorMode, setLanguage, showAppBorder, setShowAppBorder, showSourceAppIcon, setShowSourceAppIcon, compactMode, setCompactMode, checkHotkeyConflict,
        clipboardItemFontSize, setClipboardItemFontSize, clipboardTagFontSize, setClipboardTagFontSize,
        emojiPanelEnabled, setEmojiPanelEnabled, tagManagerEnabled, setTagManagerEnabled,
        setMqttEnabled, saveMqtt, setMqttServer, setMqttPort, setMqttUser, setMqttPass, setMqttTopic, setMqttProtocol, setMqttWsPath, setMqttNotificationEnabled,
        setCloudSyncEnabled, setCloudSyncAuto, setCloudSyncIntervalSec, setCloudSyncSnapshotIntervalMin, setCloudSyncWebdavUrl, setCloudSyncWebdavUsername, setCloudSyncWebdavPassword, setCloudSyncWebdavBasePath, saveCloudSync,
        setFileServerEnabled, setFileServerPort, setFileTransferAutoOpen, setShowAutoCloseHint, setFileServerAutoClose, setFileTransferAutoCopy, fetchEffectiveTransferPath,
        setShowAppSelector, handleResetSettings,
        aiEnabled, setAiEnabled, aiTargetLang, setAiTargetLang, aiThinkingBudget, setAiThinkingBudget, saveSetting,
        onOpenChat,
        aiProfiles, setAiProfiles, aiAssignedProfileTask, setAiAssignedProfileTask, aiAssignedProfileMouthpiece, setAiAssignedProfileMouthpiece, aiAssignedProfileTranslate, setAiAssignedProfileTranslate
    } = props;

    const [emailCopied, setEmailCopied] = useState(false);
    const [appVersion, setAppVersion] = useState("");
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
    const [openHints, setOpenHints] = useState<Set<string>>(new Set());
    const [privacyKindsOpen, setPrivacyKindsOpen] = useState(false);
    const [privacyRulesOpen, setPrivacyRulesOpen] = useState(false);

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

    const toggleHint = (key: string) => {
        setOpenHints(prev => {
            const next = new Set(prev);
            if (next.has(key)) next.delete(key);
            else next.add(key);
            return next;
        });
    };

    const LabelWithHint = ({ label, hint, hintKey }: { label: string; hint?: string | React.ReactNode; hintKey: string }) => (
        <div className="item-label-group">
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span className="item-label">{label}</span>
                {hint && (
                    <button
                        type="button"
                        className="hint-icon-btn"
                        title={typeof hint === 'string' ? hint : undefined}
                        onClick={(e) => {
                            e.stopPropagation();
                            toggleHint(hintKey);
                        }}
                    >
                        <HelpCircle size={12} />
                    </button>
                )}
            </div>
            {hint && openHints.has(hintKey) && (
                typeof hint === 'string' ? <span className="hint">{hint}</span> : hint
            )}
        </div>
    );

    const checkModelStatus = async (profile: AiProfile) => {
        setProfileStatuses(prev => ({ ...prev, [profile.id]: 'loading' }));
        try {
            const result = await invoke<string>("check_ai_connectivity", {
                baseUrl: profile.baseUrl,
                apiKey: profile.apiKey,
                model: profile.model
            });
            if (result === "success") {
                setProfileStatuses(prev => ({ ...prev, [profile.id]: 'success' }));
            }
        } catch (e: unknown) {
            console.error("AI Check failed:", e);
            setProfileStatuses(prev => ({ ...prev, [profile.id]: 'error' }));
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
        } finally {
            setCloudSyncNowRunning(false);
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
        setAiProfiles(newProfiles);
        saveSetting('ai_profiles', JSON.stringify(newProfiles));
        setEditingProfile(null);
    };

    const handleDeleteProfile = (id: string) => {
        // Prevent deleting presets
        if (['lc_flash_v1', 'lc_think_v1', 'lc_think_2601_v1'].includes(id)) return;

        const newProfiles = aiProfiles.filter(p => p.id !== id);
        setAiProfiles(newProfiles);
        saveSetting('ai_profiles', JSON.stringify(newProfiles));
        // Reset assignments if deleted
        if (aiAssignedProfileTask === id) { setAiAssignedProfileTask('default'); saveSetting('ai_assigned_profile_task', 'default'); }
        if (aiAssignedProfileMouthpiece === id) { setAiAssignedProfileMouthpiece('default'); saveSetting('ai_assigned_profile_mouthpiece', 'default'); }
        if (aiAssignedProfileTranslate === id) { setAiAssignedProfileTranslate('default'); saveSetting('ai_assigned_profile_translate', 'default'); }
    };



    useEffect(() => {
        getVersion()
            .then(v => setAppVersion(v))
            .catch(err => {
                console.error("Failed to get version:", err);
                setAppVersion("0.2.0");
            });

        const unlistenMqtt = listen<string>("mqtt-status", (event) => {
            settingsLog('[MQTT STATUS] Received status:', event.payload);
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
            settingsLog('[MQTT INIT] connected:', connected, 'running:', running);
            if (connected) {
                setMqttStatus("connected");
            } else if (running) {
                setMqttStatus("connecting");
            } else {
                settingsLog('[MQTT INIT] Keeping default disconnected state');
            }
        }).catch(console.error);

        return () => {
            unlistenMqtt.then(f => f());
            if (unlistenCloud) {
                unlistenCloud.then(f => f());
            }
        };
    }, []);

    const openAdvancedSettingsWindow = useCallback(async () => {
        setSettingsSubpage("advanced");
    }, [setSettingsSubpage]);

    return (
        <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            style={{ display: 'flex', flexDirection: 'column', gap: '4px', minHeight: '100%', flex: 1 }}
        >
            {settingsSubpage === "advanced" ? (
                <>
                    <AdvancedSettingsGroup
                        t={t}
                        cleanupRules={cleanupRules}
                        setCleanupRules={setCleanupRules}
                        appCleanupPolicies={appCleanupPolicies}
                        setAppCleanupPolicies={setAppCleanupPolicies}
                        installedApps={installedApps}
                    />

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
                    />

                </>
            ) : (
                <>
            {/* General Settings */}
            <GeneralSettingsGroup
                t={t}
                collapsed={collapsedGroups['general']}
                onToggle={() => toggleGroup('general')}
                LabelWithHint={LabelWithHint}
                autoStart={autoStart}
                setAutoStart={setAutoStart}
                silentStart={silentStart}
                setSilentStart={setSilentStart}
                hideTrayIcon={hideTrayIcon}
                setHideTrayIcon={setHideTrayIcon}
                edgeDocking={edgeDocking}
                setEdgeDocking={setEdgeDocking}
                followMouse={followMouse}
                setFollowMouse={setFollowMouse}
                    soundEnabled={soundEnabled}
                    setSoundEnabled={setSoundEnabled}
                    soundVolume={soundVolume}
                    setSoundVolume={setSoundVolume}
                    pasteSoundEnabled={pasteSoundEnabled}
                    setPasteSoundEnabled={setPasteSoundEnabled}
                showSearchBox={showSearchBox}
                setShowSearchBox={setShowSearchBox}
                scrollTopButtonEnabled={scrollTopButtonEnabled}
                setScrollTopButtonEnabled={setScrollTopButtonEnabled}
                emojiPanelEnabled={emojiPanelEnabled}
                setEmojiPanelEnabled={setEmojiPanelEnabled}
                tagManagerEnabled={tagManagerEnabled}
                setTagManagerEnabled={setTagManagerEnabled}
                arrowKeySelection={arrowKeySelection}
                setArrowKeySelection={setArrowKeySelection}
                saveAppSetting={saveAppSetting}
            />

            {/* Clipboard Settings */}
            <ClipboardSettingsGroup
                t={t}
                collapsed={collapsedGroups['clipboard']}
                onToggle={() => toggleGroup('clipboard')}
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
                richPasteHotkey={richPasteHotkey}
                isRecordingRich={isRecordingRich}
                setIsRecordingRich={setIsRecordingRich}
                updateRichPasteHotkey={updateRichPasteHotkey}
                searchHotkey={searchHotkey}
                isRecordingSearch={isRecordingSearch}
                setIsRecordingSearch={setIsRecordingSearch}
                updateSearchHotkey={updateSearchHotkey}
                quickPasteModifier={quickPasteModifier}
                setQuickPasteModifier={setQuickPasteModifier}
                deleteAfterPaste={deleteAfterPaste}
                setDeleteAfterPaste={setDeleteAfterPaste}
                moveToTopAfterPaste={moveToTopAfterPaste}
                setMoveToTopAfterPaste={setMoveToTopAfterPaste}
                pasteMethod={pasteMethod}
                setPasteMethod={setPasteMethod}
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
                registryWinVEnabled={registryWinVEnabled}
                setRegistryWinVEnabled={setRegistryWinVEnabled}
                isRecording={isRecording}
                setIsRecording={setIsRecording}
                hotkeyParts={hotkeyParts}
                updateHotkey={updateHotkey}
                hotkey={hotkey}
                appSettings={appSettings}
                theme={theme}
                colorMode={colorMode}
            />

            {/* Appearance Settings */}
            <AppearanceSettingsGroup
                t={t}
                collapsed={collapsedGroups['appearance']}
                onToggle={() => toggleGroup('appearance')}
                LabelWithHint={LabelWithHint}
                theme={theme}
                setTheme={setTheme}
                colorMode={colorMode}
                setColorMode={setColorMode}
                language={language}
                setLanguage={setLanguage}
                showAppBorder={showAppBorder}
                setShowAppBorder={setShowAppBorder}
                showSourceAppIcon={showSourceAppIcon}
                setShowSourceAppIcon={setShowSourceAppIcon}
                compactMode={compactMode}
                setCompactMode={setCompactMode}
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

            {/* Sync Settings */}
            <SyncSettingsGroup
                t={t}
                collapsed={collapsedGroups['sync']}
                onToggle={() => toggleGroup('sync')}
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
            />

            {CLOUD_SYNC_ENABLED && (
                <CloudSyncSettingsGroup
                    t={t}
                    collapsed={collapsedGroups['cloud_sync']}
                    onToggle={() => toggleGroup('cloud_sync')}
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
                    saveCloudSync={saveCloudSync}
                    status={cloudSyncStatus}
                    syncingNow={cloudSyncNowRunning}
                    onSyncNow={handleCloudSyncNow}
                />
            )}

            {/* AI Assistant Settings */}
            <AiSettingsGroup
                t={t}
                collapsed={collapsedGroups['ai']}
                onToggle={() => toggleGroup('ai')}
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
                aiThinkingBudget={aiThinkingBudget}
                setAiThinkingBudget={setAiThinkingBudget}
                theme={theme}
            />

            {/* File Transfer Settings */}
            <FileTransferSettingsGroup
                t={t}
                collapsed={collapsedGroups['file_transfer']}
                onToggle={() => toggleGroup('file_transfer')}
                fileServerEnabled={fileServerEnabled}
                setFileServerEnabled={setFileServerEnabled}
                fileServerPort={fileServerPort}
                setFileServerPort={setFileServerPort}
                applyFileServerPort={applyFileServerPort}
                localIp={localIp}
                availableIps={availableIps}
                setLocalIp={setLocalIp}
                actualPort={actualPort}
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

            {/* Default Apps Settings */}
            <DefaultAppsSettingsGroup
                t={t}
                collapsed={collapsedGroups['default_apps']}
                onToggle={() => toggleGroup('default_apps')}
                installedApps={installedApps}
                appSettings={appSettings}
                defaultApps={defaultApps}
                setShowAppSelector={setShowAppSelector}
            />

            {/* Data Management Settings */}
            <DataSettingsGroup
                t={t}
                collapsed={collapsedGroups['data']}
                onToggle={() => toggleGroup('data')}
                dataPath={dataPath}
            />

            <div className="settings-group">
                <button
                    type="button"
                    className="group-header settings-nav-card"
                    onClick={openAdvancedSettingsWindow}
                >
                    <div style={{ minWidth: 0, textAlign: "left" }}>
                        <h3 style={{ margin: 0 }}>{t("advanced_settings")}</h3>
                        <div className="settings-subpage-note">{t("advanced_settings_entry_desc")}</div>
                    </div>
                    <ChevronRight size={16} />
                </button>
            </div>

            <SettingsFooter
                t={t}
                appVersion={appVersion}
                updateStatus={updateStatus}
                setUpdateStatus={setUpdateStatus}
                
                onResetSettings={handleResetSettings}
                emailCopied={emailCopied}
                setEmailCopied={setEmailCopied}
            />

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
            />

                </>
            )}
        </motion.div>
    );
};

export default memo(SettingsPanel);
