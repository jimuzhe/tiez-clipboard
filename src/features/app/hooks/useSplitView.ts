import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import type { ClipboardEntry } from "../../../shared/types";
import { useWideWindow } from "../../../shared/hooks/useWideWindow";
import { useKeyboardOwnsList } from "./useKeyboardOwnsList";
import { createPointerAim } from "../../../shared/lib/pointerAim";

const ITEM_ID_PREFIX = "clipboard-item-";
/** How long a pointer heading for the pane must rest on a row before the pane switches to it. */
const HOVER_DWELL_MS = 100;

interface UseSplitViewOptions {
  enabled: boolean;
  /** Only the history list gets a detail pane (not settings, tags, emoji, transfer). */
  listVisible: boolean;
  /** Entries in display order (pinned first). */
  items: ClipboardEntry[];
  selectedItemId: number | null;
  isKeyboardMode: boolean;
  /** Entry with an open tag/note editor; the pane stays on it meanwhile. */
  lockedId: number | null;
  /** Makes a row the selection; unset when arrow-key selection is off. */
  onPointerSelect?: (index: number, id: number) => void;
}

export const useSplitView = ({ enabled, listVisible, items, selectedItemId, isKeyboardMode, lockedId, onPointerSelect }: UseSplitViewOptions) => {
  const isWide = useWideWindow();
  const [focusedId, setFocusedId] = useState<number | null>(null);
  const dwellRef = useRef<{ id: number | null; timer: number }>({ id: null, timer: 0 });
  const aimRef = useRef(createPointerAim());
  // A pointer resting on the list doesn't take the selection back while the
  // arrow keys drive it.
  const notePointer = useKeyboardOwnsList(isKeyboardMode, selectedItemId);
  const latestRef = useRef({ items, onPointerSelect, current: null as number | null });

  useEffect(() => {
    const dwell = dwellRef.current;
    return () => window.clearTimeout(dwell.timer);
  }, []);

  const active = enabled && isWide && listVisible && items.length > 0;
  latestRef.current = {
    items,
    onPointerSelect,
    current: onPointerSelect ? (isKeyboardMode ? selectedItemId : null) : focusedId
  };

  // With the pointer moving the selection there is no separate hover look.
  const pointerSelects = Boolean(onPointerSelect);
  useEffect(() => {
    document.body.classList.toggle("list-pointer-selects", pointerSelects);
  }, [pointerSelects]);

  // Lets CSS and the compact hover preview know the detail pane is showing.
  useEffect(() => {
    document.body.classList.toggle("split-view-active", active);
    document.documentElement.classList.toggle("split-view-active", active);
  }, [active]);

  // Keyboard navigation drives the pane while the arrow keys are in use.
  useEffect(() => {
    if (!isKeyboardMode || selectedItemId == null) return;
    setFocusedId(selectedItemId);
    // A hover that was about to retarget the pane is overtaken by the keys.
    const dwell = dwellRef.current;
    window.clearTimeout(dwell.timer);
    dwell.id = null;
  }, [isKeyboardMode, selectedItemId]);

  // The row under a moving pointer becomes the selection, so the arrow keys
  // go on from there and only one row is ever marked (with arrow-key selection
  // off, the pointer only picks the pane's entry). Moving up and down the list
  // selects at once. Only a pointer heading right, toward the pane, waits until
  // it rests on a row: sweeping diagonally over other rows on the way to the
  // pane's action bar must not retarget it. One delegated listener on the
  // list column.
  const handleListMouseMove = useCallback((event: MouseEvent<HTMLElement>) => {
    const towardPane = aimRef.current.note(event.clientX, event.clientY);
    if (!notePointer(event.screenX, event.screenY)) return;
    const row = (event.target as HTMLElement).closest?.(`[id^="${ITEM_ID_PREFIX}"]`);
    if (!row) return;
    const id = Number(row.id.slice(ITEM_ID_PREFIX.length));
    const dwell = dwellRef.current;
    if (!Number.isFinite(id)) return;
    // Already waiting on this row, and still on the way to the pane.
    if (dwell.id === id && towardPane) return;
    window.clearTimeout(dwell.timer);
    dwell.id = null;
    if (id === latestRef.current.current) return;
    const pick = () => {
      dwell.id = null;
      const { items: rows, onPointerSelect: select } = latestRef.current;
      const index = rows.findIndex((item) => item.id === id);
      if (select && index >= 0) select(index, id);
      else setFocusedId(id);
    };
    if (!active || !towardPane) {
      pick();
      return;
    }
    dwell.id = id;
    dwell.timer = window.setTimeout(pick, HOVER_DWELL_MS);
  }, [notePointer, active]);

  const handleListMouseLeave = useCallback(() => {
    const dwell = dwellRef.current;
    window.clearTimeout(dwell.timer);
    dwell.id = null;
    aimRef.current.reset();
  }, []);

  const detailItem = useMemo(() => {
    if (!active || items.length === 0) return null;
    const targetId = lockedId ?? focusedId;
    return items.find((item) => item.id === targetId) ?? items[0];
  }, [active, items, focusedId, lockedId]);

  return {
    splitActive: active,
    detailItem,
    handleListMouseMove,
    handleListMouseLeave,
    focusDetailItem: setFocusedId
  };
};
