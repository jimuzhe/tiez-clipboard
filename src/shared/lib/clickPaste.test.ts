import { describe, expect, test } from "vitest";
import { normalizeClickPasteMode, pasteKeepsFormat } from "./clickPaste";

describe("click paste mode", () => {
  test("by default a click keeps the format and a right-click pastes plain text", () => {
    expect(pasteKeepsFormat("left_rich", true)).toBe(true);
    expect(pasteKeepsFormat("left_rich", false)).toBe(false);
  });

  test("swapped: a click pastes plain text and a right-click keeps the format", () => {
    expect(pasteKeepsFormat("left_plain", true)).toBe(false);
    expect(pasteKeepsFormat("left_plain", false)).toBe(true);
  });

  test("an unknown or missing saved value falls back to the default", () => {
    expect(normalizeClickPasteMode(undefined)).toBe("left_rich");
    expect(normalizeClickPasteMode("")).toBe("left_rich");
    expect(normalizeClickPasteMode("left_plain")).toBe("left_plain");
  });
});
