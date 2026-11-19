import { useState } from "react";
import { DEFAULT_CLICK_PASTE_MODE, type ClickPasteMode } from "../../../shared/lib/clickPaste";
import { DEFAULT_THEME } from "../../../shared/config/themes";
import type { ClipboardEntry, Locale } from "../../../shared/types";
import type {
  AppState,
  ClipboardSortMode,
  CloudSyncContentPrefs,
  DefaultAppsMap,
  InstalledAppOption,
  QuickPasteModifier,
  SettingsSubpage
} from "../types";
import { DEFAULT_CLOUD_SYNC_CONTENT_PREFS } from "../types";
import type { AiProfile, AppCleanupPolicy } from "../../settings/types";
import { DEFAULT_COLLAPSED_GROUPS } from "../../settings/types";
import { PRESET_AI_PROFILES } from "../../settings/aiPresets";

export const useAppState = (): AppState => {
  const [showSettings, setShowSettings] = useState(false);
  const [settingsSubpage, setSettingsSubpage] = useState<SettingsSubpage>("home");
  const [showTagManager, setShowTagManager] = useState(false);
  const [tagManagerEnabled, setTagManagerEnabled] = useState(true);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>(DEFAULT_COLLAPSED_GROUPS);
  const [history, setHistory] = useState<ClipboardEntry[]>([]);
  const [clipboardSortMode, setClipboardSortMode] =
    useState<ClipboardSortMode>("activity");
  const [search, setSearch] = useState("");
  const [isComposing, setIsComposing] = useState(false);
  const [searchIsFocused, setSearchIsFocused] = useState(false);
  const [showTagFilter, setShowTagFilter] = useState(false);
  const [tagInput, setTagInput] = useState("");
  const [showEmojiPanel, setShowEmojiPanel] = useState(false);
  const [emojiFavorites, setEmojiFavorites] = useState<string[]>([]);
  const [aiOptionsOpenId, setAiOptionsOpenId] = useState<number | null>(null);
  const [editingTagsId, setEditingTagsId] = useState<number | null>(null);
  const [editingNoteId, setEditingNoteId] = useState<number | null>(null);
  const [revealedIds, setRevealedIds] = useState<Set<number>>(new Set());
  const [autoStart, setAutoStart] = useState(true);
  const [elevatedAutostart, setElevatedAutostart] = useState(false);
  const [deduplicate, setDeduplicate] = useState(true);
  const [persistent, setPersistent] = useState(true);
  const [persistentLimitEnabled, setPersistentLimitEnabled] = useState(true);
  const [persistentLimit, setPersistentLimit] = useState<number>(1000);
  const [appSettings, setAppSettings] = useState<Record<string, string>>({});
  const [defaultApps, setDefaultApps] = useState<DefaultAppsMap>({});
  const [showAppSelector, setShowAppSelector] = useState<string | null>(null);
  const [chatMode, setChatMode] = useState(false);
  const [installedApps, setInstalledApps] = useState<InstalledAppOption[]>([]);
  const [dataPath, setDataPath] = useState<string>("");
  const [hotkey, setHotkey] = useState<string>("Alt+C");
  const [sequentialHotkey, setSequentialHotkey] = useState<string>("Alt+V");
  const [plainPasteHotkey, setPlainPasteHotkey] = useState<string>("Alt+Shift+V");
  const [searchHotkey, setSearchHotkey] = useState<string>("Alt+F");
  const [quickPasteModifier, setQuickPasteModifier] =
    useState<QuickPasteModifier>("disabled");
  const [sequentialMode, setSequentialModeState] = useState(false);
  const [isRecording, setIsRecording] = useState(false);
  const [isRecordingSequential, setIsRecordingSequential] = useState(false);
  const [isRecordingPlain, setIsRecordingPlain] = useState(false);
  const [isRecordingSearch, setIsRecordingSearch] = useState(false);
  const [deleteAfterPaste, setDeleteAfterPaste] = useState(false);
  const [moveToTopAfterPaste, setMoveToTopAfterPaste] = useState(false);
  const [clickCopyOnly, setClickCopyOnly] = useState(false);
  const [clickPasteMode, setClickPasteMode] = useState<ClickPasteMode>(DEFAULT_CLICK_PASTE_MODE);
  const [privacyProtection, setPrivacyProtection] = useState(false);
  const [privacyProtectionKinds, setPrivacyProtectionKinds] = useState<string[]>([
    "phone",
    "idcard",
    "email",
    "secret"
  ]);
  const [privacyProtectionCustomRules, setPrivacyProtectionCustomRules] = useState<string>("");
  const [sensitiveMaskPrefixVisible, setSensitiveMaskPrefixVisible] = useState(3);
  const [sensitiveMaskSuffixVisible, setSensitiveMaskSuffixVisible] = useState(3);
  const [sensitiveMaskEmailDomain, setSensitiveMaskEmailDomain] = useState(false);
  const [cleanupRules, setCleanupRules] = useState<string>("");
  const [appCleanupPolicies, setAppCleanupPolicies] = useState<AppCleanupPolicy[]>([]);
  const [captureFiles, setCaptureFiles] = useState(true);
  const [captureRichText, setCaptureRichText] = useState(false);
  const [richTextSnapshotPreview, setRichTextSnapshotPreview] = useState(false);
  const [silentStart, setSilentStart] = useState(true);
  const [followMouse, setFollowMouse] = useState(false);
  const [showAppBorder, setShowAppBorder] = useState(false);
  const [winClipboardDisabled, setWinClipboardDisabled] = useState(false);
  const [registryWinVEnabled, setRegistryWinVEnabled] = useState(false);
  const [pasteMethod, setPasteMethod] = useState("auto");
  const [theme, setTheme] = useState(DEFAULT_THEME);
  const [nativePalette, setNativePalette] = useState("default");
  const [colorMode, setColorMode] = useState("system");
  const [showSourceAppIcon, setShowSourceAppIcon] = useState(true);
  const [hideUnselectedItemActions, setHideUnselectedItemActions] = useState(false);

  const [compactMode, setCompactMode] = useState(false);
  const [compactShowTime, setCompactShowTime] = useState(false);
  const [splitViewEnabled, setSplitViewEnabled] = useState(true);
  const [clipboardItemFontSize, setClipboardItemFontSize] = useState(13);
  const [clipboardTagFontSize, setClipboardTagFontSize] = useState(10);
  const [emojiPanelEnabled, setEmojiPanelEnabled] = useState(false);
  const [emojiPanelTab, setEmojiPanelTab] = useState<"emoji" | "favorites">("emoji");
  const [showHotkeyHint, setShowHotkeyHint] = useState(false);
  const [showAutoCloseHint, setShowAutoCloseHint] = useState(false);
  const [language, setLanguage] = useState<Locale>("zh");
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [isWindowPinned, setIsWindowPinned] = useState(false);
  /** Persistent preference: search bar always visible (settings toggle). */
  const [searchBoxPinned, setSearchBoxPinned] = useState(true);
  /** Transient UI visibility (may temporarily differ from pinned via scroll reveal). */
  const [showSearchBox, setShowSearchBox] = useState(true);
  const [scrollTopButtonEnabled, setScrollTopButtonEnabled] = useState(true);
  const [arrowKeySelection, setArrowKeySelection] = useState(true);
  const [lightModeEnabled, setLightModeEnabled] = useState(false);
  const [lightModeIdleMinutes, setLightModeIdleMinutes] = useState(15);
  const [hideTrayIcon, setHideTrayIcon] = useState(false);
  const [hideDockIcon, setHideDockIcon] = useState(false);
  const [edgeDocking, setEdgeDocking] = useState(false);
  const [customBackground, setCustomBackground] = useState<string>("");
  const [customBackgroundOpacity, setCustomBackgroundOpacity] = useState(45);
  const [surfaceOpacity, setSurfaceOpacity] = useState(50);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [selectedItemId, setSelectedItemId] = useState<number | null>(null);
  const [isKeyboardMode, setIsKeyboardMode] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [currentOffset, setCurrentOffset] = useState(0);
  const [mqttEnabled, setMqttEnabled] = useState(false);
  const [mqttServer, setMqttServer] = useState("");
  const [mqttPort, setMqttPort] = useState("1883");
  const [mqttUser, setMqttUser] = useState("");
  const [mqttPass, setMqttPass] = useState("");
  const [mqttTopic, setMqttTopic] = useState("");
  const [mqttProtocol, setMqttProtocol] = useState("mqtt://");
  const [mqttWsPath, setMqttWsPath] = useState("/mqtt");
  const [mqttNotificationEnabled, setMqttNotificationEnabled] = useState(true);
  const [cloudSyncEnabled, setCloudSyncEnabled] = useState(false);
  const [cloudSyncAuto, setCloudSyncAuto] = useState(true);
  const [cloudSyncProvider, setCloudSyncProvider] = useState<"http" | "webdav">("webdav");
  const [cloudSyncServer, setCloudSyncServer] = useState("");
  const [cloudSyncApiKey, setCloudSyncApiKey] = useState("");
  const [cloudSyncIntervalSec, setCloudSyncIntervalSec] = useState("120");
  const [cloudSyncSnapshotIntervalMin, setCloudSyncSnapshotIntervalMin] = useState("720");
  const [cloudSyncWebdavUrl, setCloudSyncWebdavUrl] = useState("");
  const [cloudSyncWebdavUsername, setCloudSyncWebdavUsername] = useState("");
  const [cloudSyncWebdavPassword, setCloudSyncWebdavPassword] = useState("");
  const [cloudSyncWebdavBasePath, setCloudSyncWebdavBasePath] = useState("tiez-sync");
  const [cloudSyncContentPrefs, setCloudSyncContentPrefs] =
    useState<CloudSyncContentPrefs>(DEFAULT_CLOUD_SYNC_CONTENT_PREFS);
  const [fileServerEnabled, setFileServerEnabled] = useState(false);
  const [fileServerPort, setFileServerPort] = useState("28462");
  const [localIp, setLocalIp] = useState("");
  const [availableIps, setAvailableIps] = useState<string[]>([]);
  const [actualPort, setActualPort] = useState("");
  const [fileTransferAccessToken, setFileTransferAccessToken] = useState("");
  const [fileTransferLanKey, setFileTransferLanKey] = useState("");
  const [fileTransferPath, setFileTransferPath] = useState("");
  const [fileTransferAutoOpen, setFileTransferAutoOpen] = useState(false);
  const [fileTransferAutoCopy, setFileTransferAutoCopy] = useState(false);
  const [fileServerAutoClose, setFileServerAutoClose] = useState(false);
  const [soundEnabled, setSoundEnabled] = useState(false);
  const [pasteSoundEnabled, setPasteSoundEnabled] = useState(true);
  const [soundVolume, setSoundVolume] = useState(1.0);
  const [aiEnabled, setAiEnabled] = useState(true);
  const [aiTargetLang, setAiTargetLang] = useState("zh");
  const [aiProfiles, setAiProfiles] = useState<AiProfile[]>(PRESET_AI_PROFILES);
  const [aiAssignedProfileTask, setAiAssignedProfileTask] = useState("default");
  const [aiAssignedProfileMouthpiece, setAiAssignedProfileMouthpiece] = useState("default");
  const [aiAssignedProfileTranslate, setAiAssignedProfileTranslate] = useState("default");
  const [processingAiId, setProcessingAiId] = useState<number | null>(null);
  const [typeFilter, setTypeFilter] = useState<string | null>(null);

  return {
    showSettings,
    setShowSettings,
    settingsSubpage,
    setSettingsSubpage,
    showTagManager,
    setShowTagManager,
    tagManagerEnabled,
    setTagManagerEnabled,
    collapsedGroups,
    setCollapsedGroups,
    history,
    setHistory,
    clipboardSortMode,
    setClipboardSortMode,
    search,
    setSearch,
    isComposing,
    setIsComposing,
    searchIsFocused,
    setSearchIsFocused,
    showTagFilter,
    setShowTagFilter,
    tagInput,
    setTagInput,
    showEmojiPanel,
    setShowEmojiPanel,
    emojiFavorites,
    setEmojiFavorites,
    aiOptionsOpenId,
    setAiOptionsOpenId,
    editingTagsId,
    setEditingTagsId,
    editingNoteId,
    setEditingNoteId,
    revealedIds,
    setRevealedIds,
    autoStart,
    setAutoStart,
    elevatedAutostart,
    setElevatedAutostart,
    deduplicate,
    setDeduplicate,
    persistent,
    setPersistent,
    persistentLimitEnabled,
    setPersistentLimitEnabled,
    persistentLimit,
    setPersistentLimit,
    appSettings,
    setAppSettings,
    defaultApps,
    setDefaultApps,
    showAppSelector,
    setShowAppSelector,
    chatMode,
    setChatMode,
    installedApps,
    setInstalledApps,
    dataPath,
    setDataPath,
    hotkey,
    setHotkey,
    sequentialHotkey,
    setSequentialHotkey,
    plainPasteHotkey,
    setPlainPasteHotkey,
    searchHotkey,
    setSearchHotkey,
    quickPasteModifier,
    setQuickPasteModifier,
    sequentialMode,
    setSequentialModeState,
    isRecording,
    setIsRecording,
    isRecordingSequential,
    setIsRecordingSequential,
    isRecordingPlain,
    setIsRecordingPlain,
    isRecordingSearch,
    setIsRecordingSearch,
    deleteAfterPaste,
    setDeleteAfterPaste,
    moveToTopAfterPaste,
    clickCopyOnly,
    clickPasteMode,
    setClickPasteMode,
    setMoveToTopAfterPaste,
    setClickCopyOnly,
    privacyProtection,
    setPrivacyProtection,
    privacyProtectionKinds,
    setPrivacyProtectionKinds,
    privacyProtectionCustomRules,
    setPrivacyProtectionCustomRules,
    sensitiveMaskPrefixVisible,
    setSensitiveMaskPrefixVisible,
    sensitiveMaskSuffixVisible,
    setSensitiveMaskSuffixVisible,
    sensitiveMaskEmailDomain,
    setSensitiveMaskEmailDomain,
    cleanupRules,
    setCleanupRules,
    appCleanupPolicies,
    setAppCleanupPolicies,
    captureFiles,
    setCaptureFiles,
    captureRichText,
    setCaptureRichText,
    richTextSnapshotPreview,
    setRichTextSnapshotPreview,
    silentStart,
    setSilentStart,
    followMouse,
    setFollowMouse,
    showAppBorder,
    setShowAppBorder,
    winClipboardDisabled,
    setWinClipboardDisabled,
    registryWinVEnabled,
    setRegistryWinVEnabled,
    pasteMethod,
    setPasteMethod,
    theme,
    setTheme,
    nativePalette,
    setNativePalette,
    colorMode,
    setColorMode,
    showSourceAppIcon,
    setShowSourceAppIcon,
    hideUnselectedItemActions,
    setHideUnselectedItemActions,

    compactMode,
    setCompactMode,
    compactShowTime,
    setCompactShowTime,
    splitViewEnabled,
    setSplitViewEnabled,
    clipboardItemFontSize,
    setClipboardItemFontSize,
    clipboardTagFontSize,
    setClipboardTagFontSize,
    emojiPanelEnabled,
    setEmojiPanelEnabled,
    emojiPanelTab,
    setEmojiPanelTab,
    showHotkeyHint,
    setShowHotkeyHint,
    showAutoCloseHint,
    setShowAutoCloseHint,
    language,
    setLanguage,
    settingsLoaded,
    setSettingsLoaded,
    isWindowPinned,
    setIsWindowPinned,
    searchBoxPinned,
    setSearchBoxPinned,
    showSearchBox,
    setShowSearchBox,
    scrollTopButtonEnabled,
    setScrollTopButtonEnabled,
    arrowKeySelection,
    setArrowKeySelection,
    lightModeEnabled,
    setLightModeEnabled,
    lightModeIdleMinutes,
    setLightModeIdleMinutes,
    hideTrayIcon,
    setHideTrayIcon,
    hideDockIcon,
    setHideDockIcon,
    edgeDocking,
    setEdgeDocking,

    customBackground,
    setCustomBackground,
    customBackgroundOpacity,
    setCustomBackgroundOpacity,
    surfaceOpacity,
    setSurfaceOpacity,
    selectedIndex,
    setSelectedIndex,
    selectedItemId,
    setSelectedItemId,
    isKeyboardMode,
    setIsKeyboardMode,
    isLoadingMore,
    setIsLoadingMore,
    hasMore,
    setHasMore,
    currentOffset,
    setCurrentOffset,
    mqttEnabled,
    setMqttEnabled,
    mqttServer,
    setMqttServer,
    mqttPort,
    setMqttPort,
    mqttUser,
    setMqttUser,
    mqttPass,
    setMqttPass,
    mqttTopic,
    setMqttTopic,
    mqttProtocol,
    setMqttProtocol,
    mqttWsPath,
    setMqttWsPath,
    mqttNotificationEnabled,
    setMqttNotificationEnabled,
    cloudSyncEnabled,
    setCloudSyncEnabled,
    cloudSyncAuto,
    setCloudSyncAuto,
    cloudSyncProvider,
    setCloudSyncProvider,
    cloudSyncServer,
    setCloudSyncServer,
    cloudSyncApiKey,
    setCloudSyncApiKey,
    cloudSyncIntervalSec,
    setCloudSyncIntervalSec,
    cloudSyncSnapshotIntervalMin,
    setCloudSyncSnapshotIntervalMin,
    cloudSyncWebdavUrl,
    setCloudSyncWebdavUrl,
    cloudSyncWebdavUsername,
    setCloudSyncWebdavUsername,
    cloudSyncWebdavPassword,
    setCloudSyncWebdavPassword,
    cloudSyncWebdavBasePath,
    setCloudSyncWebdavBasePath,
    cloudSyncContentPrefs,
    setCloudSyncContentPrefs,
    fileServerEnabled,
    setFileServerEnabled,
    fileServerPort,
    setFileServerPort,
    localIp,
    setLocalIp,
    availableIps,
    setAvailableIps,
    actualPort,
    setActualPort,
    fileTransferAccessToken,
    setFileTransferAccessToken,
    fileTransferLanKey,
    setFileTransferLanKey,
    fileTransferPath,
    setFileTransferPath,
    fileTransferAutoOpen,
    setFileTransferAutoOpen,
    fileTransferAutoCopy,
    setFileTransferAutoCopy,
    fileServerAutoClose,
    setFileServerAutoClose,
    soundEnabled,
    setSoundEnabled,
    pasteSoundEnabled,
    setPasteSoundEnabled,
    soundVolume,
    setSoundVolume,
    aiEnabled,
    setAiEnabled,
    aiTargetLang,
    setAiTargetLang,
    aiProfiles,
    setAiProfiles,
    aiAssignedProfileTask,
    setAiAssignedProfileTask,
    aiAssignedProfileMouthpiece,
    setAiAssignedProfileMouthpiece,
    aiAssignedProfileTranslate,
    setAiAssignedProfileTranslate,
    processingAiId,
    setProcessingAiId,
    typeFilter,
    setTypeFilter
  };
};
