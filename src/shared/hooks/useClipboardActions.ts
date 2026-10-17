import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Dispatch, RefObject, SetStateAction } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import type { ClipboardEntry } from "../types";
import type { VirtualClipboardListHandle } from "../../features/clipboard/types";
import { reconcileHistoryTags } from "../lib/historyState";

interface UseClipboardActionsOptions {
  t: (key: string) => string;
  pushToast: (msg: string, duration?: number) => number;
  deleteAfterPaste: boolean;
  moveToTopAfterPaste: boolean;
  /** Clicking (or Enter on) an item only copies it; the user pastes (#175). */
  clickCopyOnly: boolean;
  isWindowPinned: boolean;
  setSearch: (val: string) => void;
  setHistory: Dispatch<SetStateAction<ClipboardEntry[]>>;
  virtualListRef: RefObject<VirtualClipboardListHandle | null>;
}

export const useClipboardActions = ({
  t,
  pushToast,
  deleteAfterPaste,
  moveToTopAfterPaste,
  clickCopyOnly,
  isWindowPinned,
  setSearch,
  setHistory,
  virtualListRef
}: UseClipboardActionsOptions) => {
  const copyToClipboard = useCallback(
    async (
      id: number,
      content: string,
      contentType: string,
      pasteWithFormat = true,
      isPinned = false,
      tags: string[] = []
    ) => {
      try {
        if (document.activeElement instanceof HTMLElement) {
          document.activeElement.blur();
        }

        const invokeContent =
          id !== 0 &&
          (contentType === "image" || contentType === "video" || contentType === "file")
            ? ""
            : content;

        if (clickCopyOnly) {
          await invoke("copy_to_clipboard", {
            content: invokeContent,
            contentType,
            paste: false,
            id: id,
            deleteAfterUse: false,
            pasteWithFormat,
            moveToTop: false
          });
          setSearch("");
          // Out of the way so the user can paste where they want; a pinned
          // window stays, so confirm instead.
          if (isWindowPinned) {
            pushToast(t("copied_to_clipboard"), 1500);
          } else {
            await invoke("hide_window_cmd");
          }
          return;
        }

        const shouldDelete = deleteAfterPaste && !isPinned && tags.length === 0;
        await invoke("copy_to_clipboard", {
          content: invokeContent,
          contentType,
          paste: true,
          id: id,
          deleteAfterUse: shouldDelete,
          pasteWithFormat,
          moveToTop: moveToTopAfterPaste
        });

        if (moveToTopAfterPaste && !shouldDelete) {
          const now = Date.now();
          setHistory((prev) =>
            prev.map((item) =>
              item.id === id
                ? { ...item, timestamp: now, sort_at: now, last_used_at: now }
                : item
            )
          );
        }

        setSearch("");
      } catch (err) {
        // The backend event opens the actionable permission dialog.
        if (String(err).includes("accessibility_permission_required")) return;
        const errorMsg = t("copy_failed") + (err?.toString() || "");
        pushToast(errorMsg, 3000);
      }
    },
    [clickCopyOnly, deleteAfterPaste, isWindowPinned, moveToTopAfterPaste, pushToast, setHistory, setSearch, t]
  );

  const openContent = useCallback(
    async (item: ClipboardEntry) => {
      try {
        await invoke("open_content", {
          id: item.id,
          content: item.content,
          contentType: item.content_type
        });
      } catch (err) {
        const errorMsg = t("open_failed") + (err?.toString() || "");
        pushToast(errorMsg, 3000);
      }
    },
    [pushToast, t]
  );

  const deleteEntryById = useCallback(
    async (id: number): Promise<boolean> => {
      try {
        await invoke("delete_clipboard_entry", { id });
        setHistory((prev) => prev.filter((item) => item.id !== id));
        return true;
      } catch (err) {
        const errorMsg = t("delete_failed") + ": " + (err?.toString() || "");
        pushToast(errorMsg, 3000);
        return false;
      }
    },
    [pushToast, setHistory, t]
  );

  const deleteEntry = useCallback(
    async (e: ReactMouseEvent, id: number) => {
      e.stopPropagation();
      await deleteEntryById(id);
    },
    [deleteEntryById]
  );

  const togglePin = useCallback(
    async (e: ReactMouseEvent, id: number, currentPinned: boolean) => {
      e.stopPropagation();
      try {
        await invoke("toggle_clipboard_pin", { id, isPinned: !currentPinned });
        const now = Date.now();
        setHistory((prev) =>
          prev
            .map((item) =>
              item.id === id
                ? { ...item, is_pinned: !currentPinned, timestamp: now, sort_at: now }
                : item
            )
            .sort((a, b) => {
              if (a.is_pinned === b.is_pinned) {
                return (b.sort_at || b.timestamp) - (a.sort_at || a.timestamp);
              }
              return a.is_pinned ? -1 : 1;
            })
        );
      } catch (err) {
        const errorMsg =
          (currentPinned ? t("unpin_failed") : t("pin_failed")) + ": " + (err?.toString() || "");
        pushToast(errorMsg, 3000);
      }
    },
    [pushToast, setHistory, t]
  );

  const handleUpdateTags = useCallback(
    async (id: number, newTags: string[]) => {
      try {
        const newId = await invoke<number>("update_tags", { id, tags: newTags });
        setHistory((prev) =>
          reconcileHistoryTags(prev, id, newId, newTags)
        );

        setTimeout(() => {
          if (virtualListRef.current) {
            virtualListRef.current.resetAfterIndex(0);
          }
        }, 0);
      } catch (err) {
        console.error("更新标签失败", err);
        pushToast(t("save_tags_failed") + String(err), 4000);
      }
    },
    [pushToast, setHistory, t, virtualListRef]
  );

  const handleUpdateNote = useCallback(
    async (id: number, note: string) => {
      const cleaned = note.trim();
      try {
        await invoke("update_entry_note", { id, note: cleaned });
        setHistory((prev) =>
          prev.map((item) => (item.id === id ? { ...item, note: cleaned } : item))
        );
        setTimeout(() => {
          if (virtualListRef.current) {
            virtualListRef.current.resetAfterIndex(0);
          }
        }, 0);
      } catch (err) {
        console.error("更新备注失败", err);
        pushToast(t("save_note_failed") + String(err), 4000);
      }
    },
    [pushToast, setHistory, t, virtualListRef]
  );

  return {
    copyToClipboard,
    openContent,
    deleteEntry,
    deleteEntryById,
    togglePin,
    handleUpdateTags,
    handleUpdateNote
  };
};
