type CompactPreviewControls = {
  forceHide: () => void;
  warmup: () => void;
  supported: () => boolean;
  warmupSupported: () => boolean;
};

let controls: CompactPreviewControls | null = null;

/**
 * When the clipboard window appears under the cursor, WebView synthesizes
 * mouseenter. Ignore that until the user actually moves the pointer.
 */
let requirePointerMove = false;
let gateOriginX = 0;
let gateOriginY = 0;
let gateHasOrigin = false;
const POINTER_MOVE_ARM_PX = 3;

export const registerCompactPreviewControls = (next: CompactPreviewControls) => {
  controls = next;
};

export const forceHideCompactPreviewWindow = () => {
  controls?.forceHide();
};

export const warmupCompactPreviewWindow = () => {
  controls?.warmup();
};

export const isCompactPreviewWindowSupported = () =>
  controls?.supported() ?? false;

export const isCompactPreviewWarmupSupported = () =>
  controls?.warmupSupported() ?? false;

/** Call when the main clipboard window is shown (hotkey / tray / etc.). */
export const noteCompactPreviewClipboardShown = () => {
  requirePointerMove = true;
  gateHasOrigin = false;
  controls?.forceHide();
};

export const isCompactPreviewHoverArmed = () => !requirePointerMove;

/** Remember the synthetic enter point while the gate is closed. */
export const noteCompactPreviewHoverPoint = (screenX: number, screenY: number) => {
  if (!requirePointerMove) return;
  gateOriginX = screenX;
  gateOriginY = screenY;
  gateHasOrigin = true;
};

/**
 * Returns true when this move just armed hover (caller should schedule preview).
 */
export const noteCompactPreviewPointerMove = (
  screenX: number,
  screenY: number
): boolean => {
  if (!requirePointerMove) return false;

  if (!gateHasOrigin) {
    gateOriginX = screenX;
    gateOriginY = screenY;
    gateHasOrigin = true;
    return false;
  }

  const dx = Math.abs(screenX - gateOriginX);
  const dy = Math.abs(screenY - gateOriginY);
  if (dx < POINTER_MOVE_ARM_PX && dy < POINTER_MOVE_ARM_PX) {
    return false;
  }

  requirePointerMove = false;
  gateHasOrigin = false;
  return true;
};

/**
 * The payload last sent to the preview window. Tauri drops events sent before
 * a window has registered its listener, so a preview window that is still
 * loading (first hover, or after light mode rebuilt it) misses the send and
 * shows an empty card. It announces itself when ready; the list then sends
 * the remembered payload again.
 */
let lastPayload: { requestId: number; payload: unknown } | null = null;

export const rememberCompactPreviewPayload = (requestId: number, payload: unknown) => {
  lastPayload = { requestId, payload };
};

export const forgetCompactPreviewPayload = () => {
  lastPayload = null;
};

/** The remembered payload, only while it still belongs to the pending request. */
export const compactPreviewPayloadFor = <T>(requestId: number | null): T | null =>
  requestId !== null && lastPayload?.requestId === requestId ? (lastPayload.payload as T) : null;

/** Size the window is shown at when the preview hasn't reported its own yet. */
export const compactPreviewFallbackSize = (bounds: {
  maxWidth: number;
  maxHeight: number;
  minWidth: number;
}) => ({
  width: Math.min(bounds.maxWidth, Math.max(bounds.minWidth, 220)),
  height: Math.min(bounds.maxHeight, 180),
});
