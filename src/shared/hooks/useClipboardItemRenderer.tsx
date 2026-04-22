import { useCallback } from "react";
import type { Dispatch, SetStateAction, MouseEvent, ReactNode } from "react";
import type { DragControls } from "framer-motion";
import ClipboardItem from "../../features/clipboard/components/ClipboardItem";
import type { QuickPasteHint } from "../../features/clipboard/types";
import type { ClipboardEntry } from "../types";
import type { Locale } from "../types";

interface UseClipboardItemRendererOptions {
  privacyProtection: boolean;
  revealedIds: Set<number>;
  isKeyboardMode: boolean;
  selectedIndex: number;
  isWindowPinned: boolean;
  editingTagsId: number | null;
  tagInput: string;
  tagColors: Record<string, string>;
  theme: string;
  language: Locale;
  t: (key: string) => string;
  compactMode: boolean;
  showSourceAppIcon: boolean;
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
    pasteWithFormat?: boolean
  ) => Promise<void>;
  setSelectedIndex: Dispatch<SetStateAction<number>>;
  setRevealedIds: Dispatch<SetStateAction<Set<number>>>;
  openContent: (item: ClipboardEntry) => void;
  togglePin: (event: MouseEvent, id: number, isPinned: boolean) => void;
  deleteEntry: (event: MouseEvent, id: number) => void;
  setEditingTagsId: Dispatch<SetStateAction<number | null>>;
  setTagInput: Dispatch<SetStateAction<string>>;
  handleUpdateTags: (id: number, tags: string[]) => void;
  handleAIAction: (id: number, content: string, actionType: string) => void;
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
  selectedIndex,
  isWindowPinned,
  editingTagsId,
  tagInput,
  tagColors,
  theme,
  language,
  t,
  compactMode,
  showSourceAppIcon,
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
  setSelectedIndex,
  setRevealedIds,
  openContent,
  togglePin,
  deleteEntry,
  setEditingTagsId,
  setTagInput,
  handleUpdateTags,
  handleAIAction
}: UseClipboardItemRendererOptions): { renderItemContent: RenderItemContent } => {
  const renderItemContent = useCallback(
    (item: ClipboardEntry, index: number, dragControls?: DragControls, disableLayout?: boolean) => {
      const isSensitiveHidden =
        privacyProtection &&
        (item.tags?.includes("sensitive") ||
          item.tags?.includes("密码") ||
          item.tags?.includes("password")) &&
        !revealedIds.has(item.id);
      const isEditingTags = editingTagsId === item.id;

      return (
        <ClipboardItem
          id={`clipboard-item-${item.id}`}
          key={item.id}
          item={item}
          isSelected={isKeyboardMode && index === selectedIndex}
          windowPinned={isWindowPinned}
          isSensitiveHidden={!!isSensitiveHidden}
          isRevealed={revealedIds.has(item.id)}
          isEditingTags={isEditingTags}
          tagInput={isEditingTags ? tagInput : ""}
          tagColors={tagColors}
          theme={theme}
          language={language}
          t={t}
          quickPasteHint={quickPasteHintsById[item.id]}
          compactMode={compactMode}
          showSourceAppIcon={showSourceAppIcon}
          richTextSnapshotPreview={richTextSnapshotPreview}
          sensitiveMaskPrefixVisible={sensitiveMaskPrefixVisible}
          sensitiveMaskSuffixVisible={sensitiveMaskSuffixVisible}
          sensitiveMaskEmailDomain={sensitiveMaskEmailDomain}
          onSelect={() => setSelectedIndex(index)}
          onCopy={(withFormat) =>
            copyToClipboard(item.id, item.content, item.content_type, withFormat)
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
            if (editingTagsId === item.id) {
              setEditingTagsId(null);
            } else {
              setEditingTagsId(item.id);
              setTagInput("");
            }
          }}
          onTagInput={setTagInput}
          onTagAdd={() => {
            const newTag = tagInput.trim();
            if (newTag && !item.tags?.includes(newTag)) {
              handleUpdateTags(item.id, [...(item.tags || []), newTag]);
            }
            setTagInput("");
            setEditingTagsId(null);
          }}
          onTagDelete={(tag) => {
            handleUpdateTags(item.id, item.tags ? item.tags.filter((t) => t !== tag) : []);
          }}
          isAIProcessing={processingAiId === item.id}
          aiEnabled={aiEnabled}
          aiOptionsOpen={aiOptionsOpenId === item.id}
          onAIOptionsToggle={() =>
            setAiOptionsOpenId((prev) => (prev === item.id ? null : item.id))
          }
          onAIAction={(actionType) => handleAIAction(item.id, item.content, actionType)}
          onInputSubmit={(input) =>
            handleAIAction(
              item.id,
              `这是我之前的原始意图：\n"${item.content}"\n\n这是我补充的信息：\n"${input}"\n\n请结合以上信息完成任务。`,
              "task"
            )
          }
          dragControls={dragControls}
          disableLayout={disableLayout}
        />
      );
    },
    [
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
      showSourceAppIcon,
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
      setSelectedIndex,
      setRevealedIds,
      openContent,
      togglePin,
      deleteEntry,
      setEditingTagsId,
      setTagInput,
      handleUpdateTags,
      handleAIAction
    ]
  );

  return { renderItemContent };
};


