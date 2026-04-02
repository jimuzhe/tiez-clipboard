import { useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { translations } from "../../../locales";
import AdvancedSettingsGroup from "./groups/AdvancedSettingsGroup";
import { useAppState } from "../../app/hooks/useAppState";
import { useSettingsInit } from "../../../shared/hooks/useSettingsInit";
import { useSettingsPostInit } from "../../../shared/hooks/useSettingsPostInit";
import { useAppBootstrap } from "../../../shared/hooks/useAppBootstrap";
import { useSettingsApply } from "../../../shared/hooks/useSettingsApply";
import { useCustomBackground } from "../../../shared/hooks/useCustomBackground";

const AdvancedSettingsWindow = () => {
    const appState = useAppState();
    const {
        setAppSettings,
        setHotkey,
        setTheme,
        setColorMode,
        setCompactMode,
        language,
        setLanguage,
        customBackground,
        setCustomBackground,
        customBackgroundOpacity,
        setCustomBackgroundOpacity,
        surfaceOpacity,
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
        cleanupRules,
        setCleanupRules,
        appCleanupPolicies,
        setAppCleanupPolicies,
        setSilentStart,
        setFollowMouse,
        showAppBorder,
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
        setSequentialModeState,
        theme,
        colorMode,
        compactMode,
        settingsLoaded,
        clipboardItemFontSize,
        setClipboardItemFontSize,
        clipboardTagFontSize,
        setClipboardTagFontSize,
        setEmojiPanelEnabled,
        setTagManagerEnabled,
        setEmojiPanelTab,
        setEmojiFavorites,
        setAutoStart,
        setWinClipboardDisabled,
        setDefaultApps,
        setFileServerEnabled,
        setActualPort,
        setLocalIp,
        setAvailableIps,
        setDataPath,
        setInstalledApps,
        setIsWindowPinned,
        setAiEnabled,
        setAiTargetLang,
        setAiThinkingBudget,
        setAiProfiles,
        setAiAssignedProfileTask,
        setAiAssignedProfileMouthpiece,
        setAiAssignedProfileTranslate,
        setSettingsLoaded,
        setSoundEnabled,
        setSoundVolume,
        setPasteSoundEnabled,
        setPasteMethod,
        installedApps,
        setFileTransferPath
    } = appState;

    const tagManagerSizeRef = useRef<{ width: number; height: number } | null>(null);

    const t = useCallback((key: string) => {
        const k = key as keyof typeof translations["zh"];
        return translations[language][k] || translations["en"][k] || key;
    }, [language]);

    const settings = useSettingsInit({
        setAppSettings,
        setHotkey,
        setTheme,
        setColorMode,
        setCompactMode,
        setLanguage
    });

    useSettingsPostInit({
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
    });

    const fetchEffectiveTransferPath = useCallback(() => {
        invoke<string>("get_active_file_transfer_path")
            .then(setFileTransferPath)
            .catch(console.error);
    }, [setFileTransferPath]);

    useAppBootstrap({
        fetchEffectiveTransferPath,
        setDataPath,
        setInstalledApps,
        setAutoStart,
        setWinClipboardDisabled,
        setDefaultApps,
        setFileServerEnabled,
        setActualPort,
        setLocalIp,
        setAvailableIps
    });

    useSettingsApply({
        theme,
        colorMode,
        showAppBorder,
        compactMode,
        settingsLoaded,
        clipboardItemFontSize,
        clipboardTagFontSize,
        surfaceOpacity
    });

    useCustomBackground({
        customBackground,
        customBackgroundOpacity,
        theme
    });

    const LabelWithHint = useMemo(
        () => ({ label }: { label: string; hint?: string; hintKey: string }) => (
            <span className="item-label">{label}</span>
        ),
        []
    );

    return (
        <div className="advanced-settings-window-shell">
            <AdvancedSettingsGroup
                t={t}
                LabelWithHint={LabelWithHint}
                cleanupRules={cleanupRules}
                setCleanupRules={setCleanupRules}
                appCleanupPolicies={appCleanupPolicies}
                setAppCleanupPolicies={setAppCleanupPolicies}
                installedApps={installedApps}
            />
        </div>
    );
};

export default AdvancedSettingsWindow;
