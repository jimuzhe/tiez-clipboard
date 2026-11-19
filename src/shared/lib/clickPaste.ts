// Which mouse button pastes with formatting and which pastes plain text.
// "left_rich" is how it always worked: a click keeps the format, a right-click
// pastes plain text. "left_plain" swaps the two.

export type ClickPasteMode = "left_rich" | "left_plain";

export const DEFAULT_CLICK_PASTE_MODE: ClickPasteMode = "left_rich";

export const normalizeClickPasteMode = (value: unknown): ClickPasteMode =>
  value === "left_plain" ? "left_plain" : DEFAULT_CLICK_PASTE_MODE;

/**
 * Whether a paste keeps the formatting. The primary action (left click, Enter,
 * the Paste button) follows the left button; the secondary one is the
 * right-click.
 */
export const pasteKeepsFormat = (mode: ClickPasteMode, primary: boolean): boolean =>
  (mode === "left_rich") === primary;
