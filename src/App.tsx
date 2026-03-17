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
import { useMqttListener } from "./shared/hooks/useMqttListener";
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
    aiOptionsOpenId,
    setAiOptionsOpenId,
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
    chatMode,
    setChatMode,
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
    mqttEnabled,
    setMqttEnabled,
    setMqttServer,
    setMqttPort,
    setMqttUser,
    setMqttPass,
    setMqttTopic,
    setMqttProtocol,
    setMqttWsPath,
    mqttNotificationEnabled,
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
    fileServerEnabled,
    setFileServerEnabled,
    setFileServerPort,
    localIp,
    setLocalIp,
    setAvailableIps,
    actualPort,
    setActualPort,
    setFileTransferPath,
    setFileTransferAutoOpen,
    setFileTransferAutoCopy,
    setFileServerAutoClose,
    fileTransferAutoOpen,
    fileTransferAutoCopy,
    fileServerAutoClose,
    soundEnabled,
    setSoundEnabled,
    soundVolume,
    setSoundVolume,
    pasteSoundEnabled,
    setPasteSoundEnabled,
    setPasteMethod,
    aiEnabled,
    setAiEnabled,
    setAiTargetLang,
    setAiThinkingBudget,
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

  const effectiveShowEmojiPanel = showEmojiPanel && emojiPanelEnabled;
  const effectiveShowTagManager = showTagManager && tagManagerEnabled;

  const debouncedSearch = useDebounce(search, 400);
  const searchInputRef = useInputFocus<HTMLInputElement>();
  const tagColors = useTagColors();
  const virtualListRef = useRef<VirtualClipboardListHandle | null>(null);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const PAGE_SIZE = 200;
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

  const hotkeyParts = useMemo(
    () => (hotkey || t('not_set')).split('+'),
    [hotkey, t]
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
  }, [hotkey, isRecording, isRecordingSequential, isRecordingRich, isRecordingSearch, t]);


  const { toasts, pushToast, confirmDialog, openConfirm, closeConfirm } = useOverlays();

  useSoundEffects({ soundEnabled, soundVolume, pasteSoundEnabled });

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
    setSettingsLoaded
  });

  useEffect(() => {
    const unlisten = listen("focus-search-input", () => {
      setShowSettings(false);
      setShowTagManager(false);
      setChatMode(false);
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
    setChatMode,
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

  useMqttListener({ enabled: mqttNotificationEnabled, t });

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
    captureFiles,
    captureRichText,
    fileTransferAutoCopy,
    fileServerAutoClose,
    fileTransferAutoOpen,
    persistent,
    soundVolume,
    arrowKeySelection,
    setIsKeyboardMode,
    setSelectedIndex
  });

  const {
    checkHotkeyConflict,
    updateHotkey,
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

  useNavigationSync({ showSettings, showTagManager: effectiveShowTagManager, chatMode, showEmojiPanel: effectiveShowEmojiPanel });

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

  const { saveMqtt, saveCloudSync, clearHistory, handleResetSettings } = useAppActions({
    t,
    mqttEnabled,
    cloudSyncEnabled,
    openConfirm,
    closeConfirm,
    pushToast,
    fetchHistory
  });

  const { handleAIAction } = useAiActions({
    aiProfiles,
    language,
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
    chatMode,
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
    processingAiId,
    aiEnabled,
    aiOptionsOpenId,
    setAiOptionsOpenId,
    copyToClipboard,
    setSelectedIndex,
    setRevealedIds,
    openContent,
    togglePin,
    deleteEntry,
    setEditingTagsId,
    setTagInput,
    handleUpdateTags,
    handleAIAction
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
    updateRichPasteHotkey,
    updateSearchHotkey,
    saveAppSetting,
    saveSetting,
    saveMqtt,
    saveCloudSync,
    fetchEffectiveTransferPath,
    handleResetSettings,
    toggleGroup,
    onOpenChat: () => setChatMode(true),
    state: appState
  });

  return (
    <div
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
        setChatMode={setChatMode}
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
        searchIsFocused={searchIsFocused}
        setSearchIsFocused={setSearchIsFocused}
        setEditingTagsId={setEditingTagsId}
        theme={theme}
        colorMode={colorMode}
        typeFilter={typeFilter}
        setTypeFilter={setTypeFilter}
      />

      <AnnouncementSystem
        announcements={announcements}
        onDismiss={dismissAnnouncement}
      />

      <main
        className="main-content"
        style={{ overflowY: (showSettings || effectiveShowTagManager) ? 'auto' : 'hidden' }}
        onWheel={handleMainWheel}
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
          handlePinnedReorder={handlePinnedReorder}
          renderItemContent={renderItemContent}
          loadMoreHistory={loadMoreHistory}
          handleListScroll={handleListScroll}
          hasMore={effectiveHasMore}
          isLoadingMore={isLoadingMore}
          showScrollTop={showScrollTopVisible}
          onScrollTop={handleScrollTop}
        />
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
