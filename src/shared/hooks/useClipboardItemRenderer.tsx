import { useCallback } from "react";

const EMPTY_TAG_SUGGESTIONS: string[] = [];
import type { Dispatch, SetStateAction, MouseEvent, ReactNode } from "react";
import type { DragControls } from "framer-motion";
import ClipboardItem from "../../features/clipboard/components/ClipboardItem";
import { isKeyboardSelectedItem } from "../../features/clipboard/lib/itemInteraction";
import { hasSensitiveTag } from "../lib/sensitiveTags";
import type { QuickPasteHint } from "../../features/clipboard/types";
import type { ClipboardEntry } from "../types";
import type { Locale } from "../types";
import type { AiDetail } from "./useAiActions";

interface UseClipboardItemRendererOptions {
  privacyProtection: boolean;
  revealedIds: Set<number>;
  isKeyboardMode: boolean;
  selectedItemId: number | null;
  /** First search result, shown as selected until a row is picked. */
  searchTopHitId: number | null;
  /** Entry shown in the wide layout's detail pane, if any. */
  detailFocusedId: number | null;
  splitActive: boolean;
  noteDraft: string | undefined;
  setNoteDraft: (note: string) => void;
  isWindowPinned: boolean;
  editingTagsId: number | null;
  editingNoteId: number | null;
  tagInput: string;
  allTags: string[];
  tagColors: Record<string, string>;
  theme: string;
  language: Locale;
  t: (key: string) => string;
  showSourceAppIcon: boolean;
  compactMode: boolean;
  richTextSnapshotPreview: boolean;
  sensitiveMaskPrefixVisible: number;
  sensitiveMaskSuffixVisible: number;
  sensitiveMaskEmailDomain: boolean;
  quickPasteHintsById: Record<number, QuickPasteHint>;
  processingAiId: number | null;
  aiEnabled: boolean;
  aiOptionsOpenId: number | null;
  setAiOptionsOpenId: Dispatch<SetStateAction<number | null>>;
  copyToClipboard: (
    id: number,
    content: string,
    contentType: string,
    pasteWithFormat?: boolean,
    isPinned?: boolean,
    tags?: string[]
  ) => Promise<void>;
  selectItemByIndex: (index: number) => void;
  setRevealedIds: Dispatch<SetStateAction<Set<number>>>;
  openContent: (item: ClipboardEntry) => void;
  togglePin: (event: MouseEvent, id: number, isPinned: boolean) => void;
  deleteEntry: (event: MouseEvent, id: number) => void;
  setEditingTagsId: Dispatch<SetStateAction<number | null>>;
  setEditingNoteId: Dispatch<SetStateAction<number | null>>;
  setTagInput: Dispatch<SetStateAction<string>>;
  handleUpdateTags: (id: number, tags: string[]) => void;
  handleUpdateNote: (id: number, note: string) => void;
  handleAIAction: (id: number, content: string, actionType: string) => void;
  handleAISupplement: (id: number, input: string) => void;
  aiDetails: Record<number, AiDetail>;
  applyAIResult: (id: number) => Promise<void>;
  discardAIResult: (id: number) => void;
}

type RenderItemContent = (
  item: ClipboardEntry,
  index: number,
  dragControls?: DragControls,
  disableLayout?: boolean
) => ReactNode;

export const useClipboardItemRenderer = ({
  privacyProtection,
  revealedIds,
  isKeyboardMode,
  selectedItemId,
  searchTopHitId,
  detailFocusedId,
  splitActive,
  noteDraft,
  setNoteDraft,
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
}: UseClipboardItemRendererOptions): { renderItemContent: RenderItemContent } => {
  const renderItemContent = useCallback(
    (item: ClipboardEntry, index: number, dragControls?: DragControls, disableLayout?: boolean) => {
      // With 隐藏隐私 off nothing is hidden, so there is nothing to reveal.
      const canReveal = privacyProtection && hasSensitiveTag(item.tags);
      const isSensitiveHidden = canReveal && !revealedIds.has(item.id);
      const isEditingTags = !splitActive && editingTagsId === item.id;
      const isEditingNote = !splitActive && editingNoteId === item.id;
      // The detail pane offers the same choice for the entry it shows.
      const aiDetail = splitActive && detailFocusedId === item.id ? undefined : aiDetails[item.id];
      const aiIdle = aiDetail && !aiDetail.processing && !isSensitiveHidden;

      return (
        <ClipboardItem
          id={`clipboard-item-${item.id}`}
          key={item.id}
          item={item}
          navigationIndex={index}
          isSelected={isKeyboardSelectedItem(item.id, selectedItemId, isKeyboardMode) || item.id === searchTopHitId}
          isDetailFocused={detailFocusedId === item.id}
          windowPinned={isWindowPinned}
          isSensitiveHidden={!!isSensitiveHidden}
          isRevealed={revealedIds.has(item.id)}
          canReveal={canReveal}
          isEditingTags={isEditingTags}
          isEditingNote={isEditingNote}
          noteDraft={isEditingNote ? noteDraft : undefined}
          onNoteDraftChange={setNoteDraft}
          tagInput={isEditingTags ? tagInput : ""}
          tagSuggestions={isEditingTags ? allTags : EMPTY_TAG_SUGGESTIONS}
          tagColors={tagColors}
          theme={theme}
          language={language}
          t={t}
          showSourceAppIcon={showSourceAppIcon}
          compactMode={compactMode}
          richTextSnapshotPreview={richTextSnapshotPreview}
          sensitiveMaskPrefixVisible={sensitiveMaskPrefixVisible}
          sensitiveMaskSuffixVisible={sensitiveMaskSuffixVisible}
          sensitiveMaskEmailDomain={sensitiveMaskEmailDomain}
          quickPasteHint={quickPasteHintsById[item.id]}
          onSelect={() => selectItemByIndex(index)}
          onCopy={(withFormat) =>
            copyToClipboard(item.id, item.content, item.content_type, withFormat, item.is_pinned, item.tags || [])
          }
          onToggleReveal={(e) => {
            e.stopPropagation();
            setRevealedIds((prev) => {
              const next = new Set(prev);
              if (next.has(item.id)) next.delete(item.id);
              else next.add(item.id);
              return next;
            });
          }}
          onOpen={(e) => {
            e.stopPropagation();
            openContent(item);
          }}
          onTogglePin={(e) => togglePin(e, item.id, item.is_pinned)}
          onDelete={(e) => deleteEntry(e, item.id)}
          onToggleTagEditor={(e) => {
            e.stopPropagation();
            setEditingNoteId(null);
            if (editingTagsId === item.id) {
              setEditingTagsId(null);
            } else {
              setEditingTagsId(item.id);
              setTagInput("");
            }
          }}
          onToggleNoteEditor={(e) => {
            e.stopPropagation();
            setEditingTagsId(null);
            setTagInput("");
            if (editingNoteId === item.id) {
              setEditingNoteId(null);
            } else {
              setEditingNoteId(item.id);
            }
          }}
          onNoteSave={(note) => {
            handleUpdateNote(item.id, note);
            setEditingNoteId(null);
          }}
          onNoteEditCancel={() => setEditingNoteId(null)}
          onTagInput={setTagInput}
          onTagAdd={() => {
            const newTag = tagInput.trim();
            if (newTag && !item.tags?.includes(newTag)) {
              handleUpdateTags(item.id, [...(item.tags || []), newTag]);
            }
            setTagInput("");
            setEditingTagsId(null);
          }}
          onTagPick={(picked) => {
            const next = picked.trim();
            if (!next || item.tags?.includes(next)) return;
            handleUpdateTags(item.id, [...(item.tags || []), next]);
            setTagInput("");
            setEditingTagsId(null);
          }}
          onTagEditCancel={() => {
            setTagInput("");
            setEditingTagsId(null);
          }}
          onTagDelete={(tag) => {
            handleUpdateTags(item.id, item.tags ? item.tags.filter((t) => t !== tag) : []);
          }}
          // Per-item state, so a second request elsewhere keeps this spinner.
          isAIProcessing={aiDetail?.processing ?? processingAiId === item.id}
          aiEnabled={aiEnabled}
          aiOptionsOpen={aiOptionsOpenId === item.id}
          onAIOptionsToggle={() =>
            setAiOptionsOpenId((prev) => (prev === item.id ? null : item.id))
          }
          onAIAction={(actionType) => handleAIAction(item.id, item.content, actionType)}
          onInputSubmit={(input) => handleAISupplement(item.id, input)}
          aiQuestion={aiIdle ? aiDetail.question : undefined}
          aiPendingResult={aiIdle ? aiDetail.response : undefined}
          // A result from a cut-down long entry can only be copied, not applied.
          onAIApply={aiDetail?.truncated ? undefined : () => void applyAIResult(item.id)}
          onAIDiscard={() => discardAIResult(item.id)}
          dragControls={dragControls}
          disableLayout={disableLayout}
        />
      );
    },
    [
      privacyProtection,
      revealedIds,
      selectedItemId,
      searchTopHitId,
      detailFocusedId,
      splitActive,
      noteDraft,
      setNoteDraft,
      isKeyboardMode,
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
    ]
  );

  return { renderItemContent };
};
