import { useEffect, useMemo, useRef, useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ToastContainer from "./shared/components/ToastContainer";
import ConfirmDialog from "./shared/components/ConfirmDialog";

import { translations } from "./locales";
import AppHeader from "./features/app/components/AppHeader";
import AppMainContent from "./features/app/components/AppMainContent";
import { useAppState } from "./features/app/hooks/useAppState";
import { useSettingsPanelProps } from "./features/settings/hooks/useSettingsPanelProps";
import { useDebounce } from "./shared/hooks/useDebounce";
import { useHistoryFetch } from "./shared/hooks/useHistoryFetch";
import { HISTORY_PAGE_SIZE } from "./features/app/constants/pagination";
import { useHotkeyConfig } from "./shared/hooks/useHotkeyConfig";
import { useInputFocus } from "./shared/hooks/useInputFocus";
import { useSearchScroll } from "./shared/hooks/useSearchScroll";
import { useSettingsApply } from "./shared/hooks/useSettingsApply";
import { useSettingsInit } from "./shared/hooks/useSettingsInit";
import { useSettingsPostInit } from "./shared/hooks/useSettingsPostInit";
import { useSettingsSync } from "./shared/hooks/useSettingsSync";
import { useTagColors } from "./shared/hooks/useTagColors";
import { useClipboardEvents } from "./shared/hooks/useClipboardEvents";
import { useClipboardActions } from "./shared/hooks/useClipboardActions";
import { useSoundEffects } from "./shared/hooks/useSoundEffects";
import { useWindowPinnedListener } from "./shared/hooks/useWindowPinnedListener";
import { useCustomBackground } from "./shared/hooks/useCustomBackground";
import { useToastListener } from "./shared/hooks/useToastListener";
import { useAppBootstrap } from "./shared/hooks/useAppBootstrap";
import { useAppActions } from "./shared/hooks/useAppActions";
import { useNavigationSync } from "./shared/hooks/useNavigationSync";
import { useContextMenuBlock } from "./shared/hooks/useContextMenuBlock";
import { useSettingsPanelReset } from "./shared/hooks/useSettingsPanelReset";
import { useTagManagerRefresh } from "./shared/hooks/useTagManagerRefresh";
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
import type { ClipboardEntry } from "./shared/types";
import type { VirtualClipboardListHandle } from "./features/clipboard/types";

const insertHistoryItem = (list: ClipboardEntry[], item: ClipboardEntry) => {
  const next = list.slice();
  const isPinned = !!item.is_pinned;
  let insertIndex = 0;

  if (isPinned) {
    while (insertIndex < next.length) {
      const current = next[insertIndex];
      if (!current.is_pinned) break;
      if (current.timestamp < item.timestamp) break;
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
      if (current.timestamp < item.timestamp) break;
      insertIndex++;
    }
  }

  next.splice(insertIndex, 0, item);
  return next;
};

const App = () => {
  const appState = useAppState();
  const {
    showSettings,
    setShowSettings,
    showTagManager,
    setShowTagManager,
    tagManagerEnabled,
    setTagManagerEnabled,
    setCollapsedGroups,
    history,
    setHistory,
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
    editingTagsId,
    setEditingTagsId,
    revealedIds,
    setRevealedIds,
    setAutoStart,
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
    setDefaultApps,
    setInstalledApps,
    setDataPath,
    hotkey,
    setHotkey,
    sequentialHotkey,
    setSequentialHotkey,
    richPasteHotkey,
    setRichPasteHotkey,
    searchHotkey,
    setSearchHotkey,
    sequentialMode,
    setSequentialModeState,
    isRecording,
    setIsRecording,
    isRecordingSequential,
    setIsRecordingSequential,
    isRecordingRich,
    setIsRecordingRich,
    isRecordingSearch,
    setIsRecordingSearch,
    deleteAfterPaste,
    setDeleteAfterPaste,
    moveToTopAfterPaste,
    setMoveToTopAfterPaste,
    privacyProtection,
    setPrivacyProtection,
    setPrivacyProtectionKinds,
    setPrivacyProtectionCustomRules,
    captureFiles,
    setCaptureFiles,
    captureRichText,
    setCaptureRichText,
    richTextSnapshotPreview,
    setRichTextSnapshotPreview,
    setSilentStart,
    theme,
    setTheme,
    colorMode,
    setColorMode,
    showAppBorder,
    setShowAppBorder,
    compactMode,
    setCompactMode,
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
    setWinClipboardDisabled,
    setRegistryWinVEnabled,
    showSearchBox,
    setShowSearchBox,
    scrollTopButtonEnabled,
    setScrollTopButtonEnabled,
    arrowKeySelection,
    setArrowKeySelection,
    setHideTrayIcon,
    setEdgeDocking,
    setFollowMouse,
    customBackground,
    setCustomBackground,
    customBackgroundOpacity,
    setCustomBackgroundOpacity,
    surfaceOpacity,
    setSurfaceOpacity,
    selectedIndex,
    setSelectedIndex,
    isKeyboardMode,
    setIsKeyboardMode,
    isLoadingMore,
    setIsLoadingMore,
    hasMore,
    setHasMore,
    currentOffset,
    setCurrentOffset,
    soundEnabled,
    setSoundEnabled,
    soundVolume,
    setSoundVolume,
    pasteSoundEnabled,
    setPasteSoundEnabled,
    setPasteMethod,
    typeFilter,
    setTypeFilter
  } = appState;

  const effectiveShowEmojiPanel = showEmojiPanel && emojiPanelEnabled;
  const effectiveShowTagManager = showTagManager && tagManagerEnabled;

  const debouncedSearch = useDebounce(search, 400);
  const searchInputRef = useInputFocus<HTMLInputElement>();
  const tagColors = useTagColors();
  const virtualListRef = useRef<VirtualClipboardListHandle | null>(null);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const PAGE_SIZE = HISTORY_PAGE_SIZE;
  const { fetchHistory, loadMoreHistory } = useHistoryFetch({
    debouncedSearch,
    typeFilter,
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

  const { handleListScroll: handleSearchScroll, handleMainWheel } = useSearchScroll({
    showSearchBox,
    setShowSearchBox,
    search,
    showSettings,
    showTagManager: effectiveShowTagManager,
    appSettings
  });

  const showScrollTopVisible = showScrollTop && scrollTopButtonEnabled;

  const handleListScroll = useCallback((offset: number) => {
    handleSearchScroll(offset);
    setShowScrollTop(offset > 200);
  }, [handleSearchScroll]);

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

  const mainHotkeys = useMemo(
    () =>
      hotkey
        .split(/[\r\n]+/g)
        .map((item) => item.trim())
        .filter((item) => !!item),
    [hotkey]
  );

  // Compute all tags when tag manager is open OR when search box is focused
  const allTags = useMemo(() => {
    if (!effectiveShowTagManager && !showTagFilter) return [];

    const set = new Set<string>();
    // Scan history for all unique tags  
    history.forEach(item => {
      (item.tags || []).forEach(tag => set.add(tag));
    });
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [history, effectiveShowTagManager, showTagFilter]);

  useEffect(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      if (isRecording || isRecordingSequential || isRecordingRich || isRecordingSearch) return;
      if (mainHotkeys.length === 0) return;

      const activeEl = document.activeElement as HTMLElement | null;
      const isEditable = !!activeEl && (
        activeEl.tagName === 'INPUT' ||
        activeEl.tagName === 'TEXTAREA' ||
        activeEl.isContentEditable
      );

      for (const item of mainHotkeys) {
        if (matchesHotkey(event, item)) {
          event.preventDefault();
          invoke("toggle_window_cmd").catch(console.error);
          return;
        }

        if (!isEditable && item.toUpperCase().includes('WIN') && matchesHotkey(event, item, { ignoreWin: true })) {
          event.preventDefault();
          invoke("toggle_window_cmd").catch(console.error);
          return;
        }
      }
    };

    window.addEventListener('keydown', handleKeydown, true);
    return () => window.removeEventListener('keydown', handleKeydown, true);
  }, [isRecording, isRecordingSequential, isRecordingRich, isRecordingSearch, mainHotkeys]);


  const { toasts, pushToast, confirmDialog, openConfirm, closeConfirm } = useOverlays();

  useSoundEffects({ soundEnabled, soundVolume, pasteSoundEnabled });

  const { announcements, dismissAnnouncement } = useAnnouncements();

  const tagManagerSizeRef = useRef<{ width: number; height: number } | null>(null);

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
    setClipboardItemFontSize,
    setClipboardTagFontSize,
    setEmojiPanelEnabled,
    setTagManagerEnabled,
    setEmojiPanelTab,
    setEmojiFavorites,
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
    setSettingsLoaded
  });

  useEffect(() => {
    const unlisten = listen("focus-search-input", () => {
      setShowSettings(false);
      setShowTagManager(false);
      setShowEmojiPanel(false);
      setShowSearchBox(true);
      setSearchIsFocused(true);
      requestAnimationFrame(() => {
        searchInputRef.current?.focus();
      });
    });

    return () => {
      unlisten.then((off) => off());
    };
  }, [
    setShowSettings,
    setShowTagManager,
    setShowEmojiPanel,
    setShowSearchBox,
    setSearchIsFocused,
    searchInputRef
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
    setDataPath,
    setInstalledApps,
    setAutoStart,
    setWinClipboardDisabled,
    setDefaultApps
  });

  useWindowPinnedListener({
    onPinnedChange: setIsWindowPinned
  });

  useContextMenuBlock();

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

  useEffect(() => {
    fetchHistory();
  }, []);

  useToastListener({ pushToast });

  useSettingsPanelReset({ showSettings, setCollapsedGroups });

  useTagManagerRefresh({
    showTagManager: effectiveShowTagManager,
    settingsLoaded,
    persistentLimitEnabled,
    persistentLimit,
    fetchHistory
  });

  const saveAppSetting = useCallback(async (type: string, path: string) => {
    const key = `app.${type}`;
    setAppSettings(prev => ({ ...prev, [key]: path }));

    // Sync theme-related settings to localStorage for instant startup (prevents flash)
    try {
      if (type === 'theme') localStorage.setItem('tiez_theme', path);
      if (type === 'color_mode') localStorage.setItem('tiez_color_mode', path);
      if (type === 'compact_mode') localStorage.setItem('tiez_compact_mode', path);
    } catch (e) {
      // Ignore localStorage errors
    }

    try {
      await invoke("save_setting", { key, value: path });
    } catch (err) {
      console.error("保存设置失败", err);
    }
  }, [setAppSettings]);

  const saveSetting = useCallback((key: string, val: string) => {
    invoke("save_setting", { key, value: val }).catch(console.error);
  }, []);

  useSettingsSync({
    settingsLoaded,
    deduplicate,
    saveAppSetting,
    saveSetting,
    captureFiles,
    captureRichText,
    persistent,
    soundVolume,
    arrowKeySelection,
    setIsKeyboardMode,
    setSelectedIndex
  });

  const {
    checkHotkeyConflict,
    updateHotkey,
    addMainHotkey,
    removeMainHotkey,
    updateSequentialHotkey,
    updateRichPasteHotkey,
    updateSearchHotkey
  } =
    useHotkeyConfig({
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
    });

  useNavigationSync({ showSettings, showTagManager: effectiveShowTagManager, showEmojiPanel: effectiveShowEmojiPanel });

  const { copyToClipboard, openContent, deleteEntry, togglePin, handleUpdateTags } =
    useClipboardActions({
      t,
      pushToast,
      deleteAfterPaste,
      moveToTopAfterPaste,
      setSearch,
      setHistory,
      virtualListRef
    });

  const { clearHistory, handleResetSettings } = useAppActions({
    t,
    openConfirm,
    closeConfirm,
    pushToast,
    fetchHistory
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
    debouncedSearch,
    search,
    typeFilter
  });

  const effectiveHasMore = hasMore && filteredHistory.length >= PAGE_SIZE;

  const { pinnedItems, unpinnedItems, handlePinnedReorder } = usePinnedSort({
    filteredHistory,
    history,
    setHistory
  });

  useListSelectionReset({ filteredHistory, setSelectedIndex });

  useSearchFetchTrigger({ debouncedSearch, isComposing, typeFilter, fetchHistory });

  useScrollToSelection({
    filteredHistory,
    selectedIndex,
    isKeyboardMode,
    pinnedCount: pinnedItems.length,
    virtualListRef
  });

  useKeyboardNavigation({
    filteredHistory,
    selectedIndex,
    setSelectedIndex,
    isKeyboardMode,
    setIsKeyboardMode,
    showSettings,
    showTagManager: effectiveShowTagManager,
    chatMode: false,
    editingTagsId,
    arrowKeySelection,
    richPasteHotkey,
    searchInputRef,
    copyToClipboard,
    setSearch
  });


  const { renderItemContent } = useClipboardItemRenderer({
    privacyProtection,
    revealedIds,
    isKeyboardMode,
    selectedIndex,
    isWindowPinned,
    editingTagsId,
    tagInput,
    tagColors,
    theme,
    language,
    t,
    compactMode,
    richTextSnapshotPreview,
    copyToClipboard,
    setSelectedIndex,
    setRevealedIds,
    openContent,
    togglePin,
    deleteEntry,
    setEditingTagsId,
    setTagInput,
    handleUpdateTags
  });

  const settingsPanelProps = useSettingsPanelProps({
    t,
    theme,
    language,
    colorMode,
    mainHotkeys,
    checkHotkeyConflict,
    updateHotkey,
    addMainHotkey,
    removeMainHotkey,
    updateSequentialHotkey,
    updateRichPasteHotkey,
    updateSearchHotkey,
    saveAppSetting,
    saveSetting,
    handleResetSettings,
    toggleGroup,
    state: appState
  });

  const appHeaderProps = {
    t,
    showSettings,
    setShowSettings,
    showTagManager: effectiveShowTagManager,
    setShowTagManager,
    tagManagerEnabled,
    showEmojiPanel: effectiveShowEmojiPanel,
    setShowEmojiPanel,
    emojiPanelEnabled,
    isWindowPinned,
    setIsWindowPinned,
    clearHistory,
    showSearchBox,
    search,
    setSearch,
    setIsComposing,
    searchInputRef,
    showTagFilter,
    setShowTagFilter,
    allTags,
    searchIsFocused,
    setSearchIsFocused,
    setEditingTagsId,
    theme,
    typeFilter,
    setTypeFilter
  };

  const appMainContentProps = {
    t,
    theme,
    showSettings,
    showTagManager: effectiveShowTagManager,
    tagManagerEnabled,
    showEmojiPanel: effectiveShowEmojiPanel,
    settingsPanelProps,
    emojiFavorites,
    setEmojiFavorites,
    emojiPanelTab,
    setEmojiPanelTab,
    saveSetting,
    filteredHistory,
    search,
    pinnedItems,
    unpinnedItems,
    compactMode,
    selectedIndex,
    isKeyboardMode,
    virtualListRef,
    handlePinnedReorder,
    renderItemContent,
    loadMoreHistory,
    handleListScroll,
    hasMore: effectiveHasMore,
    isLoadingMore,
    showScrollTop: showScrollTopVisible,
    onScrollTop: handleScrollTop
  };

  return (
    <div
      className="app-container"
    >
      <AppHeader {...appHeaderProps} />

      <AnnouncementSystem
        announcements={announcements}
        onDismiss={dismissAnnouncement}
      />

      <main
        className="main-content"
        style={{ overflowY: (showSettings || effectiveShowTagManager) ? 'auto' : 'hidden' }}
        onWheel={handleMainWheel}
      >
        <AppMainContent {...appMainContentProps} />
      </main>

      <ToastContainer toasts={toasts} />

      <ConfirmDialog
        open={confirmDialog.show}
        title={confirmDialog.title}
        message={confirmDialog.message}
        theme={theme}
        confirmLabel={t('confirm')}
        cancelLabel={t('cancel')}
        onClose={closeConfirm}
        onConfirm={confirmDialog.onConfirm}
      />

    </div >
  );
}

export default App;
