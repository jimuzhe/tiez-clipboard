/**
 * What can sit on top of the clipboard list and take the keyboard from it.
 * While any of these is up, Up/Down, Enter, Shift+Enter, Delete and the
 * plain-paste hotkey must not move, paste, copy or delete list entries.
 */
export interface ListCover {
  showSettings: boolean;
  showTagManager: boolean;
  chatMode: boolean;
  showEmojiPanel: boolean;
  /** A tag or note editor is open on an entry. */
  editingInline: boolean;
  /** A modal dialog (confirm, update) is open. */
  dialogOpen: boolean;
  /** A text field other than the search box has focus (e.g. an AI follow-up). */
  textFieldFocused: boolean;
}

export const isListCovered = (cover: ListCover): boolean =>
  cover.showSettings ||
  cover.showTagManager ||
  cover.chatMode ||
  cover.showEmojiPanel ||
  cover.editingInline ||
  cover.dialogOpen ||
  cover.textFieldFocused;

/** A modal dialog is open (AppModal marks its panel aria-modal). */
export const hasOpenModal = () => document.querySelector("[aria-modal='true']") !== null;

/** The element is a text field other than the search box; keys typed there stay there. */
export const isOtherTextField = (element: EventTarget | null, searchInput: HTMLElement | null) =>
  element instanceof HTMLElement &&
  element !== searchInput &&
  (element.tagName === "INPUT" || element.tagName === "TEXTAREA" || element.isContentEditable);
