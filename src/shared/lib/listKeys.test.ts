import { expect, test } from "vitest";
import { isListCovered, type ListCover } from "./listKeys";

const uncovered: ListCover = {
  showSettings: false,
  showTagManager: false,
  chatMode: false,
  showEmojiPanel: false,
  editingInline: false,
  dialogOpen: false,
  textFieldFocused: false
};

test("list keys work when nothing covers the list", () => {
  expect(isListCovered(uncovered)).toBe(false);
});

test("any panel, editor, dialog or other text field takes the list keys", () => {
  for (const key of Object.keys(uncovered) as (keyof ListCover)[]) {
    expect(isListCovered({ ...uncovered, [key]: true })).toBe(true);
  }
});

test("the emoji panel and a confirm dialog cover the list", () => {
  expect(isListCovered({ ...uncovered, showEmojiPanel: true })).toBe(true);
  expect(isListCovered({ ...uncovered, dialogOpen: true })).toBe(true);
});
