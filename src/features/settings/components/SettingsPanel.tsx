import { memo, useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { HelpCircle } from "lucide-react";
import { motion } from "framer-motion";
import type { Locale } from "../../../shared/types";
import type { DefaultAppsMap, InstalledAppOption } from "../../app/types";
import AppSelectorModal from "./AppSelectorModal";
import UpdateModal from "./UpdateModal";
import type { UpdateModalData } from "../types";
import GeneralSettingsGroup from "./groups/GeneralSettingsGroup";
import ClipboardSettingsGroup from "./groups/ClipboardSettingsGroup";
import AppearanceSettingsGroup from "./groups/AppearanceSettingsGroup";
import DefaultAppsSettingsGroup from "./groups/DefaultAppsSettingsGroup";
import DataSettingsGroup from "./groups/DataSettingsGroup";
import SettingsFooter from "./SettingsFooter";

interface SettingsPanelProps {
    t: (key: string) => string;
    theme: string;
    language: Locale;
    colorMode: string;
    clipboardItemFontSize: number;
    setClipboardItemFontSize: (val: number) => void;
    clipboardTagFontSize: number;
    setClipboardTagFontSize: (val: number) => void;
    collapsedGroups: Record<string, boolean>;
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
    privacyProtection: boolean;
    privacyProtectionKinds: string[];
    setPrivacyProtectionKinds: (val: string[]) => void;
    privacyProtectionCustomRules: string;
    setPrivacyProtectionCustomRules: (val: string) => void;
    hotkey: string;
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
    installedApps: InstalledAppOption[];
    appSettings: Record<string, string>;
    defaultApps: DefaultAppsMap;
    showAppSelector: string | null;
    dataPath: string;
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
    setPrivacyProtection: (val: boolean) => void;
    setIsRecording: (val: boolean) => void;
    isRecording: boolean;
    mainHotkeys: string[];
    updateHotkey: (key: string) => void;
    addMainHotkey: (key: string, options?: { skipAvailabilityCheck?: boolean }) => Promise<boolean>;
    removeMainHotkey: (key: string) => Promise<boolean>;
    setTheme: (val: string) => void;
    setColorMode: (val: string) => void;
    setLanguage: (val: Locale) => void;
    showAppBorder: boolean;
    setShowAppBorder: (val: boolean) => void;
    compactMode: boolean;
    setCompactMode: (val: boolean) => void;
    checkHotkeyConflict: (newHotkey: string, mode: "main" | "sequential" | "rich" | "search") => boolean;
    setShowAppSelector: (val: string | null) => void;
    handleResetSettings: () => void;
}

const SettingsPanel = (props: SettingsPanelProps) => {
    const {
        t, theme, language, colorMode,
        collapsedGroups, autoStart, silentStart, persistent, persistentLimitEnabled, persistentLimit, deduplicate, captureFiles, captureRichText, richTextSnapshotPreview, deleteAfterPaste, moveToTopAfterPaste,
        sequentialMode, sequentialHotkey, isRecordingSequential,
        richPasteHotkey, isRecordingRich, searchHotkey, isRecordingSearch,
        privacyProtection, privacyProtectionKinds, setPrivacyProtectionKinds, privacyProtectionCustomRules, setPrivacyProtectionCustomRules, registryWinVEnabled, setRegistryWinVEnabled, showSearchBox, setShowSearchBox, scrollTopButtonEnabled, setScrollTopButtonEnabled, arrowKeySelection, setArrowKeySelection,
        soundEnabled, setSoundEnabled, pasteSoundEnabled, setPasteSoundEnabled,
        soundVolume, setSoundVolume,
        pasteMethod, setPasteMethod,
        hideTrayIcon, setHideTrayIcon,
        edgeDocking, setEdgeDocking,
        followMouse, setFollowMouse,
        customBackground, setCustomBackground,
        customBackgroundOpacity, setCustomBackgroundOpacity,
        surfaceOpacity, setSurfaceOpacity,
        installedApps, appSettings, defaultApps, showAppSelector, dataPath,
        toggleGroup, setAutoStart, setSilentStart, setPersistent, setPersistentLimitEnabled, setPersistentLimit, setDeduplicate, setCaptureFiles, setCaptureRichText, setRichTextSnapshotPreview, setDeleteAfterPaste, setMoveToTopAfterPaste, saveAppSetting,
        setSequentialModeState, setIsRecordingSequential, updateSequentialHotkey,
        setIsRecordingRich, updateRichPasteHotkey,
        setIsRecordingSearch, updateSearchHotkey,
        setPrivacyProtection,
        setIsRecording, isRecording, hotkey, mainHotkeys, updateHotkey, addMainHotkey, removeMainHotkey,
        setTheme, setColorMode, setLanguage, showAppBorder, setShowAppBorder, compactMode, setCompactMode, checkHotkeyConflict,
        clipboardItemFontSize, setClipboardItemFontSize, clipboardTagFontSize, setClipboardTagFontSize,
        emojiPanelEnabled, setEmojiPanelEnabled, tagManagerEnabled, setTagManagerEnabled,
        setShowAppSelector, handleResetSettings
    } = props;

    const [emailCopied, setEmailCopied] = useState(false);
    const [appVersion, setAppVersion] = useState("");
    const [updateStatus, setUpdateStatus] = useState<string>("");
    const [updateModalData, setUpdateModalData] = useState<UpdateModalData | null>(null);
    const [openHints, setOpenHints] = useState<Set<string>>(new Set());
    const [privacyKindsOpen, setPrivacyKindsOpen] = useState(false);
    const [privacyRulesOpen, setPrivacyRulesOpen] = useState(false);

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
            <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                <span className="item-label">{label}</span>
                {hint && (
                    <button
                        type="button"
                        className="hint-icon-btn"
                        title={typeof hint === "string" ? hint : undefined}
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
                typeof hint === "string" ? <span className="hint">{hint}</span> : hint
            )}
        </div>
    );

    useEffect(() => {
        getVersion()
            .then(v => setAppVersion(v))
            .catch(err => {
                console.error("Failed to get version:", err);
                setAppVersion("0.2.0");
            });
    }, []);

    return (
        <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            style={{ display: "flex", flexDirection: "column", gap: "4px" }}
        >
            <GeneralSettingsGroup
                t={t}
                collapsed={collapsedGroups["general"]}
                onToggle={() => toggleGroup("general")}
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

            <ClipboardSettingsGroup
                t={t}
                collapsed={collapsedGroups["clipboard"]}
                onToggle={() => toggleGroup("clipboard")}
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
                privacyKindsOpen={privacyKindsOpen}
                setPrivacyKindsOpen={setPrivacyKindsOpen}
                privacyRulesOpen={privacyRulesOpen}
                setPrivacyRulesOpen={setPrivacyRulesOpen}
                registryWinVEnabled={registryWinVEnabled}
                setRegistryWinVEnabled={setRegistryWinVEnabled}
                isRecording={isRecording}
                setIsRecording={setIsRecording}
                mainHotkeys={mainHotkeys}
                updateHotkey={updateHotkey}
                addMainHotkey={addMainHotkey}
                removeMainHotkey={removeMainHotkey}
                hotkey={hotkey}
                appSettings={appSettings}
                theme={theme}
                colorMode={colorMode}
            />

            <AppearanceSettingsGroup
                t={t}
                collapsed={collapsedGroups["appearance"]}
                onToggle={() => toggleGroup("appearance")}
                LabelWithHint={LabelWithHint}
                theme={theme}
                setTheme={setTheme}
                colorMode={colorMode}
                setColorMode={setColorMode}
                language={language}
                setLanguage={setLanguage}
                showAppBorder={showAppBorder}
                setShowAppBorder={setShowAppBorder}
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

            <DefaultAppsSettingsGroup
                t={t}
                collapsed={collapsedGroups["default_apps"]}
                onToggle={() => toggleGroup("default_apps")}
                installedApps={installedApps}
                appSettings={appSettings}
                defaultApps={defaultApps}
                setShowAppSelector={setShowAppSelector}
            />

            <DataSettingsGroup
                t={t}
                collapsed={collapsedGroups["data"]}
                onToggle={() => toggleGroup("data")}
                dataPath={dataPath}
            />

            <SettingsFooter
                t={t}
                appVersion={appVersion}
                updateStatus={updateStatus}
                setUpdateStatus={setUpdateStatus}
                setUpdateModalData={setUpdateModalData}
                onResetSettings={handleResetSettings}
                emailCopied={emailCopied}
                setEmailCopied={setEmailCopied}
            />

            <AppSelectorModal
                show={showAppSelector}
                installedApps={installedApps}
                t={t}
                onClose={() => setShowAppSelector(null)}
                onSave={saveAppSetting}
            />

            <UpdateModal
                data={updateModalData}
                t={t}
                onClose={() => setUpdateModalData(null)}
                setUpdateStatus={setUpdateStatus}
            />
        </motion.div>
    );
};

export default memo(SettingsPanel);
