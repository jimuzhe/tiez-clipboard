import type { MouseEvent, ReactNode } from "react";
import type { DragControls } from "framer-motion";
import type { ClipboardEntry, Locale } from "../../shared/types";

export interface QuickPasteHint {
  slot: string;
  combo: string;
}

export interface ClipboardItemProps {
  item: ClipboardEntry;
  isSelected: boolean;
  windowPinned: boolean;
  isSensitiveHidden: boolean;
  isRevealed: boolean;
  isEditingTags: boolean;
  tagInput: string;
  theme: string;
  language: Locale;
  t: (key: string) => string;
  quickPasteHint?: QuickPasteHint;
  showSourceAppIcon?: boolean;
  isAIProcessing?: boolean;
  aiEnabled?: boolean;
  tagColors?: Record<string, string>;
  aiOptionsOpen?: boolean;
  richTextSnapshotPreview?: boolean;
  sensitiveMaskPrefixVisible?: number;
  sensitiveMaskSuffixVisible?: number;
  sensitiveMaskEmailDomain?: boolean;

  onSelect: () => void;
  onCopy: (withFormat?: boolean) => void;
  onToggleReveal: (e: MouseEvent) => void;
  onOpen: (e: MouseEvent) => void;
  onTogglePin: (e: MouseEvent) => void;
  onDelete: (e: MouseEvent) => void;
  onToggleTagEditor: (e: MouseEvent) => void;
  onTagInput: (val: string) => void;
  onTagAdd: () => void;
  onTagDelete: (tag: string) => void;
  onAIAction?: (type: string) => void;
  onAIOptionsToggle?: () => void;
  onInputSubmit?: (val: string) => void;
  dragControls?: DragControls;
  id?: string;
  disableLayout?: boolean;
}

export type ClipboardRenderItem = (
  item: ClipboardEntry,
  index: number,
  isFirst: boolean
) => ReactNode;

export interface VirtualClipboardListProps {
  items: ClipboardEntry[];
  renderItem: ClipboardRenderItem;
  onLoadMore?: () => void;
  hasMore: boolean;
  isLoading: boolean;
  selectedIndex: number;
  isKeyboardMode: boolean;
  onScroll?: (offset: number) => void;
  compactMode: boolean;
  header?: ReactNode;
}

export interface VirtualClipboardListHandle {
  scrollToItem: (index: number) => void;
  scrollToTop: () => void;
  resetAfterIndex: (index: number) => void;
}
