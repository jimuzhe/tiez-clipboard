import { test } from "vitest";
import assert from "node:assert/strict";
import { isKeyboardSelectedItem, isItemInteractionActive, searchTopHitId } from "./itemInteraction.ts";

test("the default first-item target is not visibly selected", () => {
  assert.equal(isKeyboardSelectedItem(1, 1, false), false);
});

test("a remembered mouse-click target does not retain selection", () => {
  assert.equal(isItemInteractionActive(isKeyboardSelectedItem(2, 2, false), false, false, false), false);
});

test("keyboard navigation highlights only its selected target", () => {
  assert.equal(isKeyboardSelectedItem(2, 2, true), true);
  assert.equal(isKeyboardSelectedItem(1, 2, true), false);
  assert.equal(isKeyboardSelectedItem(1, null, true), false);
});

test("keyboard selection survives without hover", () => {
  assert.equal(isItemInteractionActive(true, false, false, false), true);
});

test("tag and note editors remain active without hover", () => {
  assert.equal(isItemInteractionActive(false, true, false, false), true);
  assert.equal(isItemInteractionActive(false, false, true, false), true);
});

test("AI options remain usable without hover", () => {
  assert.equal(isItemInteractionActive(false, false, false, true), true);
});

test("closing an editor restores the inactive state", () => {
  assert.equal(isItemInteractionActive(false, false, false, false), false);
});

const typed = {
  searchFocused: true,
  search: "abc",
  listIsCurrent: true,
  isComposing: false,
  isKeyboardMode: false,
  firstItemId: 7,
};

test("after a search the first result is the paste target", () => {
  assert.equal(searchTopHitId(typed), 7);
});

test("no top hit without search text or outside the search box", () => {
  assert.equal(searchTopHitId({ ...typed, search: "  " }), null);
  assert.equal(searchTopHitId({ ...typed, searchFocused: false }), null);
  assert.equal(searchTopHitId({ ...typed, firstItemId: null }), null);
});

test("a row picked with the keys or the mouse replaces the top hit", () => {
  assert.equal(searchTopHitId({ ...typed, isKeyboardMode: true }), null);
});

test("no top hit while the list still shows an older search or the IME is composing", () => {
  assert.equal(searchTopHitId({ ...typed, listIsCurrent: false }), null);
  assert.equal(searchTopHitId({ ...typed, isComposing: true }), null);
});
