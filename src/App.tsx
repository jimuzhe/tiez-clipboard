import { useEffect, useMemo, useRef, useCallback, useState, useDeferredValue } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ToastContainer from "./shared/components/ToastContainer";
import MacAccessibilityPrompt from "./shared/components/MacAccessibilityPrompt";
import ConfirmDialog from "./shared/components/ConfirmDialog";

import { translations } from "./locales";
import AppHeader from "./features/app/components/AppHeader";
import AppMainContent from "./features/app/components/AppMainContent";
import { useAppState } from "./features/app/hooks/useAppState";
import { useSettingsPanelProps } from "./features/settings/hooks/useSettingsPanelProps";
import { useDebounce } from "./shared/hooks/useDebounce";
import { useHistoryFetch } from "./shared/hooks/useHistoryFetch";
import { useHotkeyConfig } from "./shared/hooks/useHotkeyConfig";
import { useFocusCoordinator } from "./shared/hooks/useFocusCoordinator";
import { focusClipboardWindow } from "./shared/lib/focus";
import { useSearchScroll } from "./shared/hooks/useSearchScroll";
import { useSettingsApply } from "./shared/hooks/useSettingsApply";
import { useSettingsInit } from "./shared/hooks/useSettingsInit";
import { useSettingsPostInit } from "./shared/hooks/useSettingsPostInit";
import { useSettingsSync } from "./shared/hooks/useSettingsSync";
import { useTagColors } from "./shared/hooks/useTagColors";
import { useClipboardEvents } from "./shared/hooks/useClipboardEvents";
import { useClipboardActions } from "./shared/hooks/useClipboardActions";
import { useWindowPinnedListener } from "./shared/hooks/useWindowPinnedListener";
import { useCustomBackground } from "./shared/hooks/useCustomBackground";
import { useToastListener } from "./shared/hooks/useToastListener";
import { useAppBootstrap } from "./shared/hooks/useAppBootstrap";
import { useAppActions } from "./shared/hooks/useAppActions";
import { useNavigationSync } from "./shared/hooks/useNavigationSync";
import { useContextMenuBlock } from "./shared/hooks/useContextMenuBlock";
import { useSettingsPanelReset } from "./shared/hooks/useSettingsPanelReset";
import { useTagManagerRefresh } from "./shared/hooks/useTagManagerRefresh";
import { useAiActions } from "./shared/hooks/useAiActions";
import { matchesHotkey } from "./shared/hooks/useHotkeyMatching";
import { usePinnedSort } from "./shared/hooks/usePinnedSort";
import { useFilteredHistory } from "./shared/hooks/useFilteredHistory";
import { useKeyboardNavigation } from "./shared/hooks/useKeyboardNavigation";
import { useListSelectionReset } from "./shared/hooks/useListSelectionReset";
import { useSearchFetchTrigger } from "./shared/hooks/useSearchFetchTrigger";
import { useScrollToSelection } from "./shared/hooks/useScrollToSelection";
import { useClipboardItemRenderer } from "./shared/hooks/useClipboardItemRenderer";
import { AnnouncementSystem } from "./shared/components/Announcement";
import { useAnnouncements } from "./shared/hooks/useAnnouncements";
import { useOverlays } from "./shared/hooks/useOverlays";
import { useAutoUpdate } from "./shared/hooks/useAutoUpdate";
import UpdateDialog from "./shared/components/UpdateDialog";
import type { ClipboardEntry } from "./shared/types";
import type { QuickPasteHint, VirtualClipboardListHandle } from "./features/clipboard/types";

/** Must match privacy blur checks in `useClipboardItemRenderer` / `ClipboardItem`. */
const BUILTIN_SENSITIVE_TAG_NAMES = ["sensitive", "密码", "password"] as const;
import type { ClipboardSortMode, QuickPasteModifier } from "./features/app/types";
import {
  forceHideCompactPreviewWindow,
  isCompactPreviewWindowSupported,
  isCompactPreviewWarmupSupported,
  noteCompactPreviewClipboardShown,
  warmupCompactPreviewWindow
} from "./features/clipboard/lib/compactPreviewControls";
import { isMacPlatform } from "./shared/lib/platform";
import { isTauriRuntime } from "./shared/lib/tauriRuntime";
import { getCurrentWindow } from "@tauri-apps/api/window";
import ClipboardDetailPane from "./features/clipboard/components/ClipboardDetailPane";
import { useSplitView } from "./features/app/hooks/useSplitView";
import { useDetailPaneActions } from "./features/app/hooks/useDetailPaneActions";
import { usePaneWidth } from "./shared/hooks/usePaneWidth";
import PaneDivider from "./shared/components/PaneDivider";

const SENSITIVE_TAGS = ["sensitive", "密码", "password"];

const insertHistoryItem = (list: ClipboardEntry[], item: ClipboardEntry) => {
  const next = list.slice();
  const isPinned = !!item.is_pinned;
  let insertIndex = 0;

  if (isPinned) {
    while (insertIndex < next.length) {
      const current = next[insertIndex];
      if (!current.is_pinned) break;
      if ((current.sort_at || current.timestamp) < (item.sort_at || item.timestamp)) break;
      insertIndex++;
    }
  } else {
    while (insertIndex < next.length && next[insertIndex].is_pinned) {
      insertIndex++;
    }
    while (insertIndex < next.length) {
      const current = next[insertIndex];
      if (current.is_pinned) {
        insertIndex++;
        continue;
      }
      if ((current.sort_at || current.timestamp) < (item.sort_at || item.timestamp)) break;
      insertIndex++;
    }
  }

  next.splice(insertIndex, 0, item);
  return next;
};

const QUICK_PASTE_KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] as const;

const buildQuickPasteHintsById = (
  items: ClipboardEntry[],
  quickPasteModifier: QuickPasteModifier
): Record<number, QuickPasteHint> => {
  if (quickPasteModifier === "disabled") {
    return {};
  }

  const modifierLabels: Record<Exclude<QuickPasteModifier, "disabled">, string> = isMacPlatform()
    ? {
        ctrl: "⌃",
        alt: "⌥",
        shift: "⇧",
        win: "⌘"
      }
    : {
        ctrl: "Ctrl+",
        alt: "Alt+",
        shift: "Shift+",
        win: "Win+"
      };
  const pinnedItems = items.filter((item) => item.is_pinned).slice(0, QUICK_PASTE_KEYS.length);

  return pinnedItems.reduce<Record<number, QuickPasteHint>>((acc, item, index) => {
    acc[item.id] = {
      slot: index + 1,
      combo: `${modifierLabels[quickPasteModifier]}${QUICK_PASTE_KEYS[index]}`
    };
    return acc;
  }, {});
};

const App = () => {
  useFocusCoordinator();
  type FileTransferSourceView = "clipboard" | "settings" | "tag_manager" | "emoji_panel";

  const appState = useAppState();
  const {
    showSettings,
    setShowSettings,
    settingsSubpage,
    setSettingsSubpage,
    showTagManager,
    setShowTagManager,
    tagManagerEnabled,
    setTagManagerEnabled,
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
    setAutoStart,
    setElevatedAutostart,
    deduplicate,
    setDeduplicate,
    persistent,
    setPersistent,
    persistentLimitEnabled,
    setPersistentLimitEnabled,
    persistentLimit,
    setPersistentLimit,
    setAppSettings,
    setDefaultApps,
    chatMode,
    setChatMode,
    setInstalledApps,
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
    setMoveToTopAfterPaste,
    setClickCopyOnly,
    privacyProtection,
    setPrivacyProtection,
    sensitiveMaskPrefixVisible,
    setSensitiveMaskPrefixVisible,
    sensitiveMaskSuffixVisible,
    setSensitiveMaskSuffixVisible,
    sensitiveMaskEmailDomain,
    setSensitiveMaskEmailDomain,
    setPrivacyProtectionKinds,
    setPrivacyProtectionCustomRules,
    setCleanupRules,
    setAppCleanupPolicies,
    captureFiles,
    setCaptureFiles,
    captureRichText,
    setCaptureRichText,
    richTextSnapshotPreview,
    setRichTextSnapshotPreview,
    setSilentStart,
    setFollowMouse,
    showAppBorder,
    setShowAppBorder,
    setRegistryWinVEnabled,
    setPasteMethod,
    setWinClipboardDisabled,
    theme,
    setTheme,
    setNativePalette,
    colorMode,
    nativePalette,
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
    setLightModeEnabled,
    setLightModeIdleMinutes,
    setHideTrayIcon,
    setHideDockIcon,
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
    setMqttServer,
    setMqttPort,
    setMqttUser,
    setMqttPass,
    setMqttTopic,
    setMqttProtocol,
    setMqttWsPath,
    setMqttNotificationEnabled,
    cloudSyncEnabled,
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
    setCloudSyncContentPrefs,
    fileServerEnabled,
    setFileServerEnabled,
    setFileServerPort,
    localIp,
    setLocalIp,
    setAvailableIps,
    actualPort,
    setActualPort,
    fileTransferAccessToken,
    setFileTransferAccessToken,
    fileTransferLanKey,
    setFileTransferLanKey,
    setFileTransferPath,
    setFileTransferAutoOpen,
    setFileTransferAutoCopy,
    setFileServerAutoClose,
    fileTransferAutoOpen,
    fileTransferAutoCopy,
    fileServerAutoClose,
    setSoundEnabled,
    setPasteSoundEnabled,
    soundVolume,
    setSoundVolume,
    aiEnabled,
    setAiEnabled,
    setAiTargetLang,
    aiProfiles,
    setAiProfiles,
    setAiAssignedProfileTask,
    setAiAssignedProfileMouthpiece,
    setAiAssignedProfileTranslate,
    processingAiId,
    setProcessingAiId,
    typeFilter,
    setTypeFilter
  } = appState;

  // --- Auto Update Logic ---
  const {
    isOpen: isUpdateOpen,
    status: updateStatus,
    version: updateVersion,
    notes: updateNotes,
    important: updateImportant,
    offeredChannel: updateOfferedChannel,
    downloadProgress: updateProgress,
    onStartDownload,
    onApplyUpdate,
    onClose: closeUpdateDialog,
  } = useAutoUpdate();
  // -------------------------

  const effectiveShowEmojiPanel = showEmojiPanel && emojiPanelEnabled;
  const effectiveShowTagManager = showTagManager && tagManagerEnabled;
  const [fileTransferSourceView, setFileTransferSourceView] =
    useState<FileTransferSourceView>("clipboard");

  const debouncedSearch = useDebounce(search, 200);
  const deferredSearch = useDeferredValue(search);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const showTagFilterRef = useRef(showTagFilter);
  showTagFilterRef.current = showTagFilter;

  const dismissSearchTagFilter = useCallback(() => {
    setShowTagFilter(false);
    setSearchIsFocused(false);
    if (searchInputRef.current && document.activeElement === searchInputRef.current) {
      searchInputRef.current.blur();
    }
  }, [setShowTagFilter, setSearchIsFocused, searchInputRef]);

  useEffect(() => {
    if (!showSearchBox) {
      dismissSearchTagFilter();
    }
  }, [showSearchBox, dismissSearchTagFilter]);
  const tagColors = useTagColors();
  const virtualListRef = useRef<VirtualClipboardListHandle | null>(null);
  const appContainerRef = useRef<HTMLDivElement | null>(null);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const quickPasteHintsById = useMemo(
    () => buildQuickPasteHintsById(history, quickPasteModifier),
    [history, quickPasteModifier]
  );
  const PAGE_SIZE = 80;
  const { fetchHistory, loadMoreHistory } = useHistoryFetch({
    debouncedSearch,
    typeFilter,
    sortMode: clipboardSortMode,
    persistentLimitEnabled,
    persistentLimit,
    pageSize: PAGE_SIZE,
    currentOffset,
    historyLength: history.length,
    setHistory,
    setCurrentOffset,
    setHasMore,
    isLoadingMore,
    hasMore,
    setIsLoadingMore
  });

  const t = useCallback((key: string) => {
    const k = key as keyof typeof translations['zh'];
    return translations[language][k] || translations['en'][k] || key;
  }, [language]);

  const { handleListScroll: handleSearchScroll } = useSearchScroll({
    containerRef: appContainerRef,
    searchBoxPinned,
    showSearchBox,
    setShowSearchBox,
    search,
    showSettings,
    showTagManager: effectiveShowTagManager,
    dismissSearchTagFilter
  });

  const showScrollTopVisible = showScrollTop && scrollTopButtonEnabled;

  const getCurrentSourceView = useCallback((): FileTransferSourceView => {
    if (effectiveShowTagManager) return "tag_manager";
    if (effectiveShowEmojiPanel) return "emoji_panel";
    if (showSettings) return "settings";
    return "clipboard";
  }, [effectiveShowEmojiPanel, effectiveShowTagManager, showSettings]);

  const restoreViewAfterChat = useCallback(
    (sourceView: FileTransferSourceView) => {
      setShowTagManager(sourceView === "tag_manager");
      setShowEmojiPanel(sourceView === "emoji_panel");
      setShowSettings(sourceView === "settings");
    },
    [setShowEmojiPanel, setShowSettings, setShowTagManager]
  );

  const openFileTransfer = useCallback(() => {
    const sourceView = getCurrentSourceView();
    setFileTransferSourceView(sourceView);
    setShowTagManager(false);
    setShowEmojiPanel(false);
    setShowSettings(true);
    setChatMode(true);
  }, [getCurrentSourceView, setChatMode, setShowEmojiPanel, setShowSettings, setShowTagManager]);

  const closeFileTransfer = useCallback(() => {
    setChatMode(false);
    restoreViewAfterChat(fileTransferSourceView);
  }, [fileTransferSourceView, restoreViewAfterChat, setChatMode]);

  const handleHeaderBack = useCallback(() => {
    if (chatMode) {
      closeFileTransfer();
      return;
    }
    if (effectiveShowEmojiPanel) {
      setShowEmojiPanel(false);
      return;
    }
    if (effectiveShowTagManager) {
      setShowTagManager(false);
      return;
    }
    if (showSettings) {
      if (settingsSubpage !== "home") {
        setSettingsSubpage("home");
        return;
      }
      setShowSettings(false);
    }
  }, [
    chatMode,
    closeFileTransfer,
    effectiveShowEmojiPanel,
    effectiveShowTagManager,
    setShowEmojiPanel,
    setShowSettings,
    setSettingsSubpage,
    setShowTagManager,
    settingsSubpage,
    showSettings
  ]);

  const handleToggleHeaderChat = useCallback(() => {
    if (chatMode) {
      closeFileTransfer();
      return;
    }
    openFileTransfer();
  }, [chatMode, closeFileTransfer, openFileTransfer]);

  const handleListScroll = useCallback((offset: number) => {
    handleSearchScroll(offset);
    if (offset > 0 && showTagFilterRef.current) {
      dismissSearchTagFilter();
    }
    setShowScrollTop((prev) => {
      const next = offset > 200;
      return prev === next ? prev : next;
    });
  }, [handleSearchScroll, dismissSearchTagFilter]);

  const handleScrollTop = useCallback(() => {
    if (virtualListRef.current?.scrollToTop) {
      virtualListRef.current.scrollToTop();
      return;
    }
    virtualListRef.current?.scrollToItem(0);
  }, []);

  const toggleGroup = (group: string) => {
    setCollapsedGroups(prev => ({
      ...prev,
      [group]: !prev[group],
    }));
  };

  const hotkeyParts = useMemo(
    () => (hotkey || '').split('+').map((part) => part.trim()).filter(Boolean),
    [hotkey]
  );

  // Compute all tags when tag manager / tag filter is open, or while editing an item's tags (quick-pick list)
  const allTags = useMemo(() => {
    if (!effectiveShowTagManager && !showTagFilter && editingTagsId === null) return [];

    const set = new Set<string>();
    for (const tag of BUILTIN_SENSITIVE_TAG_NAMES) {
      set.add(tag);
    }
    history.forEach((item) => {
      (item.tags || []).forEach((tag) => set.add(tag));
    });
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [history, effectiveShowTagManager, showTagFilter, editingTagsId]);

  useEffect(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      if (isRecording || isRecordingSequential || isRecordingPlain || isRecordingSearch) return;
      if (!hotkey || hotkey === t('not_set')) return;

      const activeEl = document.activeElement as HTMLElement | null;
      const isEditable = !!activeEl && (
        activeEl.tagName === 'INPUT' ||
        activeEl.tagName === 'TEXTAREA' ||
        activeEl.isContentEditable
      );

      if (matchesHotkey(event, hotkey)) {
        event.preventDefault();
        invoke("toggle_window_cmd").catch(console.error);
        return;
      }

      if (!isEditable && hotkey.toUpperCase().includes('WIN') && matchesHotkey(event, hotkey, { ignoreWin: true })) {
        event.preventDefault();
        invoke("toggle_window_cmd").catch(console.error);
      }
    };

    window.addEventListener('keydown', handleKeydown, true);
    return () => window.removeEventListener('keydown', handleKeydown, true);
  }, [hotkey, isRecording, isRecordingSequential, isRecordingPlain, isRecordingSearch, t]);


  const { toasts, pushToast, confirmDialog, openConfirm, closeConfirm } = useOverlays(t);


  const fetchEffectiveTransferPath = useCallback(() => {
    invoke<string>("get_active_file_transfer_path")
      .then(setFileTransferPath)
      .catch(console.error);
  }, [setFileTransferPath]);

  const { announcements, dismissAnnouncement } = useAnnouncements();

  const tagManagerSizeRef = useRef<{ width: number; height: number } | null>(null);

  const settings = useSettingsInit({
    setAppSettings,
    setHotkey,
    setTheme,
    setNativePalette,
    setColorMode,
    setCompactMode,
    setLanguage
  });

  // After light mode rebuilds the WebView, Rust shows the window only once this
  // arrives, so the first frame is the themed UI rather than an empty shell.
  // Effects run while the window is still hidden (unlike requestAnimationFrame).
  useEffect(() => {
    if (!settings || !isTauriRuntime()) return;
    invoke("main_ui_ready").catch(() => { /* older backend: it times out instead */ });
  }, [settings]);

  useSettingsPostInit({
    settings,
    tagManagerSizeRef,
    setCustomBackground,
    setCustomBackgroundOpacity,
    setSurfaceOpacity,
    setClipboardItemFontSize,
    setClipboardTagFontSize,
    setEmojiPanelEnabled,
    setTagManagerEnabled,
    setEmojiPanelTab,
    setEmojiFavorites,
    setPersistent,
    setPersistentLimitEnabled,
    setPersistentLimit,
    setClipboardSortMode,
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
    setRegistryWinVEnabled,
    setPasteMethod,
    setShowSourceAppIcon,
    setHideUnselectedItemActions,
    setCompactShowTime,
    setSplitViewEnabled,

    setDeleteAfterPaste,
    setMoveToTopAfterPaste,
    setClickCopyOnly,
    setHideTrayIcon,
    setHideDockIcon,
    setEdgeDocking,
    setSearchBoxPinned,
    setShowSearchBox,
    setScrollTopButtonEnabled,
    setArrowKeySelection,
    setLightModeEnabled,
    setLightModeIdleMinutes,
    setMqttEnabled,
    setMqttServer,
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
    setCloudSyncContentPrefs,
    setFileServerAutoClose,
    setFileTransferAutoOpen,
    setFileTransferAutoCopy,
    setFileServerPort,
    setSequentialHotkey,
    setPlainPasteHotkey,
    setSearchHotkey,
    setQuickPasteModifier,
    setSequentialModeState,
    setSoundEnabled,
    setPasteSoundEnabled,
    setSoundVolume,
    setAiEnabled,
    setAiTargetLang,
    setIsWindowPinned,
    setAiProfiles,
    setAiAssignedProfileTask,
    setAiAssignedProfileMouthpiece,
    setAiAssignedProfileTranslate,
    setSettingsLoaded
  });

  const focusSearchBox = useCallback(() => {
    setShowSettings(false);
    setShowTagManager(false);
    setChatMode(false);
    setShowEmojiPanel(false);
    setShowSearchBox(true);
    setSearchIsFocused(true);
    focusClipboardWindow()
      .catch(console.error)
      .finally(() => {
        requestAnimationFrame(() => {
          searchInputRef.current?.focus();
        });
      });
  }, [
    setShowSettings,
    setShowTagManager,
    setChatMode,
    setShowEmojiPanel,
    setShowSearchBox,
    setSearchIsFocused,
    searchInputRef
  ]);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    let disposed = false;
    const unlistenPromise = listen("focus-search", () => {
      if (disposed) return;
      focusSearchBox();
    });

    return () => {
      disposed = true;
      unlistenPromise.then((unlisten) => unlisten()).catch(() => { });
    };
  }, [focusSearchBox]);

  useEffect(() => {
    const handleSearchKeydown = (event: KeyboardEvent) => {
      if (isRecording || isRecordingSequential || isRecordingPlain || isRecordingSearch) return;
      if (!searchHotkey || searchHotkey === t('not_set')) return;
      if (matchesHotkey(event, searchHotkey)) {
        event.preventDefault();
        focusSearchBox();
      }
    };

    window.addEventListener('keydown', handleSearchKeydown, true);
    return () => window.removeEventListener('keydown', handleSearchKeydown, true);
  }, [searchHotkey, isRecording, isRecordingSequential, isRecordingPlain, isRecordingSearch, t, focusSearchBox]);

  // Ctrl/Cmd+F → TieZ search (browser find is already blocked globally; this routes it).
  useEffect(() => {
    const onFind = (event: KeyboardEvent) => {
      if (isRecording || isRecordingSequential || isRecordingPlain || isRecordingSearch) return;
      if (showSettings || effectiveShowTagManager || chatMode || effectiveShowEmojiPanel) return;

      const key = event.key.toLowerCase();
      const isFind =
        ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && key === "f") ||
        (key === "f3" && !event.ctrlKey && !event.metaKey && !event.altKey);
      if (!isFind) return;

      event.preventDefault();
      event.stopPropagation();
      focusSearchBox();
    };

    window.addEventListener("keydown", onFind, true);
    return () => window.removeEventListener("keydown", onFind, true);
  }, [
    isRecording,
    isRecordingSequential,
    isRecordingPlain,
    isRecordingSearch,
    showSettings,
    effectiveShowTagManager,
    chatMode,
    effectiveShowEmojiPanel,
    focusSearchBox
  ]);

  useEffect(() => {
    if (!emojiPanelEnabled && showEmojiPanel) {
      setShowEmojiPanel(false);
    }
  }, [emojiPanelEnabled, showEmojiPanel, setShowEmojiPanel]);

  useEffect(() => {
    if (!tagManagerEnabled && showTagManager) {
      setShowTagManager(false);
    }
  }, [tagManagerEnabled, showTagManager, setShowTagManager]);

  useAppBootstrap({
    fetchEffectiveTransferPath,
    setDataPath,
    setInstalledApps,
    setAutoStart,
    setElevatedAutostart,
    setDefaultApps,
    setFileServerEnabled,
    setActualPort,
    setFileTransferAccessToken,
    setFileTransferLanKey,
    setLocalIp,
    setAvailableIps,
    setWinClipboardDisabled
  });

  useWindowPinnedListener({
    onPinnedChange: setIsWindowPinned
  });

  useContextMenuBlock();

  useSettingsApply({
    theme,
    nativePalette,
    colorMode,

    compactMode,
    compactShowTime,
    hideUnselectedItemActions,
    settingsLoaded,
    clipboardItemFontSize,
    clipboardTagFontSize,
    surfaceOpacity,
    showAppBorder
  });

  // Pre-warm compact preview window only where warmup is safe.
  // macOS keeps hover preview enabled but skips warmup to reduce UI stalls.
  useEffect(() => {
    if (!compactMode || !isCompactPreviewWindowSupported() || !isCompactPreviewWarmupSupported()) return;
    const timer = setTimeout(() => {
      warmupCompactPreviewWindow();
    }, 2000); // 2s delay: avoids impacting app startup performance
    return () => clearTimeout(timer);
  }, [compactMode]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unlistenForceHide = listen("force-hide-compact-preview", () => {
      forceHideCompactPreviewWindow();
    });
    // Opening under the cursor synthesizes mouseenter — arm the move gate
    // and hide any preview that may already be mid-flight.
    const onShown = () => {
      noteCompactPreviewClipboardShown();
    };
    // Each opening starts unfiltered: a type filter is for digging out one
    // item, not a mode to keep (#142). A pinned window never reopens, so its
    // filter stays.
    const onClipboardShown = () => {
      onShown();
      setTypeFilter(null);
    };
    const unlistenClipboardShown = listen("clipboard-shown", onClipboardShown);
    const unlistenShow = getCurrentWindow().listen("tauri://show", onShown);
    return () => {
      unlistenForceHide.then((off) => off());
      unlistenClipboardShown.then((off) => off());
      unlistenShow.then((off) => off()).catch(() => {});
    };
  }, [setTypeFilter]);

  useCustomBackground({ customBackground, customBackgroundOpacity, theme });

  useClipboardEvents({
    onUpdated: (updatedItem) => {
      setHistory(prev => {
        const withoutItem = prev.filter(item => item.id !== updatedItem.id);
        return insertHistoryItem(withoutItem, updatedItem);
      });
    },
    onRemoved: (id) => {
      setHistory(prev => prev.filter(item => item.id !== id));
    },
    onChanged: () => {
      fetchHistory(true);
    }
  });


  useToastListener({ pushToast });

  useSettingsPanelReset({ showSettings, setCollapsedGroups, setSettingsSubpage });

  useTagManagerRefresh({
    showTagManager: effectiveShowTagManager,
    settingsLoaded,
    persistentLimitEnabled,
    persistentLimit,
    fetchHistory
  });

  const saveAppSetting = useCallback(async (type: string, path: string) => {
    const key = `app.${type}`;
    console.log(`[THEME DEBUG] saveAppSetting called: key=${key}, value=${path}`);
    setAppSettings(prev => ({ ...prev, [key]: path }));

    // Sync theme-related settings to localStorage for instant startup (prevents flash)
    try {
      if (type === 'native_palette') localStorage.setItem('tiez_native_palette', path);
      if (type === 'theme') localStorage.setItem('tiez_theme', path);
      if (type === 'color_mode') localStorage.setItem('tiez_color_mode', path);
      if (type === 'compact_mode') localStorage.setItem('tiez_compact_mode', path);
      if (type === 'compact_show_time') localStorage.setItem('tiez_compact_show_time', path);
      if (type === 'language') localStorage.setItem('tiez_language', path);
    } catch (e) {
      // Ignore localStorage errors
    }

    try {
      await invoke("save_setting", { key, value: path });
      console.log(`[THEME DEBUG] saveAppSetting success: key=${key}`);
    } catch (err) {
      console.error("保存设置失败", err);
    }
  }, [setAppSettings]);

  const saveSetting = useCallback((key: string, val: string) => {
    invoke("save_setting", { key, value: val })
      .then(() => {
        if (key === "app.emoji_favorites") {
          return invoke("request_cloud_sync");
        }
        return undefined;
      })
      .catch(console.error);
  }, []);

  const updateClipboardSortMode = useCallback(
    (mode: ClipboardSortMode) => {
      setClipboardSortMode(mode);
      saveSetting("app.clipboard_sort_mode", mode);
    },
    [saveSetting, setClipboardSortMode]
  );

  useSettingsSync({
    settingsLoaded,
    deduplicate,
    saveAppSetting,
    captureFiles,
    captureRichText,
    fileTransferAutoCopy,
    fileServerAutoClose,
    fileTransferAutoOpen,
    persistent,
    arrowKeySelection,
    soundVolume,
    setIsKeyboardMode,
    setSelectedIndex
  });

  const {
    checkHotkeyConflict,
    updateHotkey,
    updateSequentialHotkey,
    updatePlainPasteHotkey,
    updateSearchHotkey
  } =
    useHotkeyConfig({
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
    });

  useNavigationSync({ showSettings, showTagManager: effectiveShowTagManager, chatMode, showEmojiPanel: effectiveShowEmojiPanel });

  const { copyToClipboard, openContent, deleteEntry, deleteEntryById, togglePin, handleUpdateTags, handleUpdateNote } =
    useClipboardActions({
      t,
      pushToast,
      deleteAfterPaste,
      moveToTopAfterPaste,
      clickCopyOnly,
      isWindowPinned,
      setSearch,
      setHistory,
      virtualListRef
    });

  const { saveMqtt, saveCloudSync, clearHistory, handleResetSettings } = useAppActions({
    t,
    mqttEnabled,
    cloudSyncEnabled,
    openConfirm,
    closeConfirm,
    pushToast,
    fetchHistory
  });

  const { handleAIAction, handleAISupplement, applyAIResult, discardAIResult, copyAIText, aiDetails } = useAiActions({
    history,
    aiProfiles,
    t,
    pushToast,
    setShowSettings,
    setProcessingAiId,
    setHistory
  });

  /* 
  const updateItemContent = async (id: number, newContent: string) => {
    try {
      await invoke("update_item_content", { id, newContent });
      // Local state will be refreshed by fetchHistory triggered by clipboard-changed event
    } catch (err) {
      console.error("Failed to update item content", err);
    }
  };
  */

  const filteredHistory = useFilteredHistory({
    history,
    search: deferredSearch,
    typeFilter,
    sortMode: clipboardSortMode
  });

  const effectiveHasMore = hasMore && filteredHistory.length >= PAGE_SIZE;

  const {
    pinnedItems,
    unpinnedItems,
    navigableHistory,
    pinnedOrderIds,
    isDraggingPinned,
    handlePinnedIdsReorder,
    handlePinnedDragStart,
    handlePinnedDragEnd
  } = usePinnedSort({
    filteredHistory,
    history,
    setHistory
  });

  const { splitActive, detailItem, handleListMouseOver, handleListMouseLeave, focusDetailItem } = useSplitView({
    enabled: splitViewEnabled,
    listVisible: !showSettings && !effectiveShowTagManager && !showEmojiPanel && !chatMode,
    items: navigableHistory,
    selectedItemId,
    isKeyboardMode,
    lockedId: editingTagsId ?? editingNoteId
  });
  const [noteDraftState, setNoteDraftState] = useState<{ id: number; text: string } | null>(null);
  const editingNoteDraft = noteDraftState?.id === editingNoteId ? noteDraftState.text : undefined;
  const setEditingNoteDraft = useCallback((text: string) => {
    if (editingNoteId !== null) setNoteDraftState({ id: editingNoteId, text });
  }, [editingNoteId]);
  useEffect(() => {
    if (editingNoteId === null) setNoteDraftState(null);
  }, [editingNoteId]);
  const mainRef = useRef<HTMLElement | null>(null);
  const splitListRef = useRef<HTMLDivElement | null>(null);
  // Stored as the list's share of the width so the split survives window resizes.
  const { value: splitRatio, resizing: splitResizing, dividerProps: splitDividerProps } = usePaneWidth({
    storageKey: "app.pane.clipboard_split",
    containerRef: mainRef,
    paneRef: splitListRef,
    min: 240,
    minRest: 280,
    unit: "ratio"
  });
  const detailIsSensitive = !!detailItem && SENSITIVE_TAGS.some((tag) => detailItem.tags?.includes(tag));
  const detailIsRevealed = !!detailItem && revealedIds.has(detailItem.id);
  const detailActions = useDetailPaneActions({
    items: navigableHistory,
    focusDetailItem,
    copyToClipboard,
    openContent,
    togglePin,
    deleteEntry,
    handleAIAction,
    handleAISupplement,
    applyAIResult,
    discardAIResult,
    copyAIText,
    setEditingTagsId,
    setEditingNoteId,
    setTagInput,
    setRevealedIds,
    handleUpdateTags,
    handleUpdateNote
  });

  useListSelectionReset({
    filteredHistory: navigableHistory,
    selectedItemId,
    setSelectedIndex,
    setSelectedItemId
  });

  useSearchFetchTrigger({ debouncedSearch, isComposing, typeFilter, sortMode: clipboardSortMode, fetchHistory });

  useScrollToSelection({
    filteredHistory: navigableHistory,
    selectedIndex,
    isKeyboardMode,
    pinnedCount: pinnedItems.length,
    virtualListRef
  });

  // Keyboard delete skips pinned entries: a stray keypress shouldn't remove
  // something the user chose to keep.
  const deleteFromKeyboard = useCallback(async (item: ClipboardEntry) => {
    if (item.is_pinned) {
      pushToast(t("keyboard_delete_pinned"), 2500);
      return false;
    }
    return deleteEntryById(item.id);
  }, [deleteEntryById, pushToast, t]);

  const { selectItemByIndex } = useKeyboardNavigation({
    filteredHistory: navigableHistory,
    selectedIndex,
    selectedItemId,
    setSelectedIndex,
    setSelectedItemId,
    isKeyboardMode,
    setIsKeyboardMode,
    showSettings,
    showTagManager: effectiveShowTagManager,
    chatMode,
    editingTagsId,
    editingNoteId,
    arrowKeySelection,
    plainPasteHotkey,
    searchInputRef,
    copyToClipboard,
    setSearch,
    deleteItem: deleteFromKeyboard
  });


  const { renderItemContent } = useClipboardItemRenderer({
    privacyProtection,
    revealedIds,
    isKeyboardMode,
    selectedItemId,
    detailFocusedId: detailItem?.id ?? null,
    splitActive,
    noteDraft: editingNoteDraft,
    setNoteDraft: setEditingNoteDraft,
    isWindowPinned,
    editingTagsId,
    editingNoteId,
    tagInput,
    allTags,
    tagColors,
    theme,
    language,
    t,
    showSourceAppIcon,
    compactMode,
    richTextSnapshotPreview,
    sensitiveMaskPrefixVisible,
    sensitiveMaskSuffixVisible,
    sensitiveMaskEmailDomain,
    quickPasteHintsById,
    processingAiId,
    aiEnabled,
    aiOptionsOpenId,
    setAiOptionsOpenId,
    copyToClipboard,
    selectItemByIndex,
    setRevealedIds,
    openContent,
    togglePin,
    deleteEntry,
    setEditingTagsId,
    setEditingNoteId,
    setTagInput,
    handleUpdateTags,
    handleUpdateNote,
    handleAIAction,
    handleAISupplement,
    aiDetails,
    applyAIResult,
    discardAIResult
  });

  const settingsPanelProps = useSettingsPanelProps({
    t,
    theme,
    language,
    colorMode,
    hotkeyParts,
    checkHotkeyConflict,
    updateHotkey,
    updateSequentialHotkey,
    updatePlainPasteHotkey,
    updateSearchHotkey,
    saveAppSetting,
    saveSetting,
    saveMqtt,
    saveCloudSync,
    fetchEffectiveTransferPath,
    handleResetSettings,
    toggleGroup,
    onOpenChat: openFileTransfer,
    state: appState
  });

  return (
    <div
      ref={appContainerRef}
      className="app-container"
    >
      <AppHeader
        t={t}
        showSettings={showSettings}
        setShowSettings={setShowSettings}
        showTagManager={effectiveShowTagManager}
        setShowTagManager={setShowTagManager}
        tagManagerEnabled={tagManagerEnabled}
        showEmojiPanel={effectiveShowEmojiPanel}
        setShowEmojiPanel={setShowEmojiPanel}
        emojiPanelEnabled={emojiPanelEnabled}
        chatMode={chatMode}
        fileServerEnabled={fileServerEnabled}
        isWindowPinned={isWindowPinned}
        setIsWindowPinned={setIsWindowPinned}
        clearHistory={clearHistory}
        showSearchBox={showSearchBox}
        search={search}
        setSearch={setSearch}
        setIsComposing={setIsComposing}
        searchInputRef={searchInputRef}
        showTagFilter={showTagFilter}
        setShowTagFilter={setShowTagFilter}
        allTags={allTags}
        tagColors={tagColors}
        searchIsFocused={searchIsFocused}
        setSearchIsFocused={setSearchIsFocused}
        setEditingTagsId={setEditingTagsId}
        theme={theme}
        colorMode={colorMode}
        settingsTitle={
          showSettings && settingsSubpage === "advanced"
            ? t("advanced_settings")
            : t("settings")
        }
        typeFilter={typeFilter}
        setTypeFilter={setTypeFilter}
        clipboardSortMode={clipboardSortMode}
        onClipboardSortModeChange={updateClipboardSortMode}
        onBack={handleHeaderBack}
        onToggleChat={handleToggleHeaderChat}
      />

      <AnnouncementSystem
        announcements={announcements}
        onDismiss={dismissAnnouncement}
      />

      <main
        ref={mainRef}
        className={`main-content${chatMode ? " file-transfer-mode" : ""}${effectiveShowTagManager ? " tag-manager-mode" : ""}${showSettings ? " settings-mode" : ""}${splitActive ? " is-split" : ""}`}
        style={{
          padding: effectiveShowTagManager ? "0" : undefined,
          ...(splitRatio != null
            ? { ["--split-columns" as string]: `minmax(240px, ${splitRatio * 100}fr) minmax(0, ${(1 - splitRatio) * 100}fr)` }
            : {})
        }}
      >
        <div
          ref={splitListRef}
          className="main-split-list"
          onMouseOver={splitActive ? handleListMouseOver : undefined}
          onMouseLeave={splitActive ? handleListMouseLeave : undefined}
        >
          <AppMainContent
            t={t}
            theme={theme}
            showSettings={showSettings}
            showTagManager={effectiveShowTagManager}
            tagManagerEnabled={tagManagerEnabled}
            showEmojiPanel={effectiveShowEmojiPanel}
            chatMode={chatMode}
            localIp={localIp}
            actualPort={actualPort}
            fileTransferAccessToken={fileTransferAccessToken}
            fileTransferLanKey={fileTransferLanKey}
            settingsPanelProps={settingsPanelProps}
            emojiFavorites={emojiFavorites}
            setEmojiFavorites={setEmojiFavorites}
            emojiPanelTab={emojiPanelTab}
            setEmojiPanelTab={setEmojiPanelTab}
            saveSetting={saveSetting}
            filteredHistory={filteredHistory}
            search={search}
            pinnedItems={pinnedItems}
            unpinnedItems={unpinnedItems}
            compactMode={compactMode}
            selectedIndex={selectedIndex}
            isKeyboardMode={isKeyboardMode}
            virtualListRef={virtualListRef}
            pinnedOrderIds={pinnedOrderIds}
            isDraggingPinned={isDraggingPinned}
            handlePinnedIdsReorder={handlePinnedIdsReorder}
            handlePinnedDragStart={handlePinnedDragStart}
            handlePinnedDragEnd={handlePinnedDragEnd}
            renderItemContent={renderItemContent}
            loadMoreHistory={loadMoreHistory}
            handleListScroll={handleListScroll}
            hasMore={effectiveHasMore}
            isLoadingMore={isLoadingMore}
            showScrollTop={showScrollTopVisible}
            onScrollTop={handleScrollTop}
          />
        </div>
        {/* Always mounted as the grid's second column; content only when wide. */}
        <aside className="main-split-detail" aria-hidden={!splitActive}>
          {splitActive && (
            <PaneDivider overlay active={splitResizing} label={t("pane_divider_hint")} {...splitDividerProps} />
          )}
          {splitActive && (
            <ClipboardDetailPane
              item={detailItem}
              t={t}
              language={language}
              actions={detailActions}
              isSensitive={detailIsSensitive}
              isSensitiveHidden={detailIsSensitive && privacyProtection && !detailIsRevealed}
              isRevealed={detailIsRevealed}
              isEditingTags={!!detailItem && editingTagsId === detailItem.id}
              isEditingNote={!!detailItem && editingNoteId === detailItem.id}
              aiEnabled={aiEnabled}
              isAIProcessing={!!detailItem && (aiDetails[detailItem.id]?.processing ?? processingAiId === detailItem.id)}
              aiDetail={detailItem ? aiDetails[detailItem.id] : undefined}
              showSourceAppIcon={showSourceAppIcon}
              tagInput={tagInput}
              tagSuggestions={allTags}
              onTagInput={setTagInput}
              noteDraft={editingNoteDraft}
              onNoteDraftChange={setEditingNoteDraft}
            />
          )}
        </aside>
      </main>

      <ToastContainer toasts={toasts} />
      <MacAccessibilityPrompt ready={settingsLoaded} theme={theme} t={t} pushToast={pushToast} />

      <ConfirmDialog
        open={confirmDialog.show}
        title={confirmDialog.title}
        message={confirmDialog.message}
        theme={theme}
        danger={confirmDialog.danger}
        confirmLabel={t('confirm')}
        cancelLabel={t('cancel')}
        onClose={closeConfirm}
        onConfirm={confirmDialog.onConfirm}
      />

      <UpdateDialog
        isOpen={isUpdateOpen}
        version={updateVersion}
        notes={updateNotes}
        important={updateImportant}
        releaseChannel={updateOfferedChannel}
        downloadProgress={updateProgress}
        status={updateStatus}
        onUpdate={updateStatus === "ready" ? onApplyUpdate : onStartDownload}
        onClose={closeUpdateDialog}
        t={t}
      />
    </div >
  );
}

export default App;
