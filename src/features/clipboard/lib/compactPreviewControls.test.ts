import { describe, expect, it } from "vitest";
import {
  compactPreviewFallbackSize,
  compactPreviewPayloadFor,
  forgetCompactPreviewPayload,
  rememberCompactPreviewPayload,
  noteCompactPreviewClipboardShown,
  noteCompactPreviewHoverPoint,
  noteCompactPreviewPointerMove,
  isCompactPreviewHoverArmed,
  registerCompactPreviewControls,
} from "./compactPreviewControls";

describe("Compact Preview Window Controls and Synthetic Pointer Gate", () => {
  it("ignores synthetic pointer enters until user physically moves the cursor", () => {
    noteCompactPreviewClipboardShown();
    expect(isCompactPreviewHoverArmed()).toBe(false);

    // Initial enter point
    noteCompactPreviewHoverPoint(100, 100);
    expect(isCompactPreviewHoverArmed()).toBe(false);

    // Micro jitter less than 3px threshold
    const armedSmall = noteCompactPreviewPointerMove(101, 101);
    expect(armedSmall).toBe(false);
    expect(isCompactPreviewHoverArmed()).toBe(false);

    // Distinct physical movement >= 3px
    const armedReal = noteCompactPreviewPointerMove(105, 105);
    expect(armedReal).toBe(true);
    expect(isCompactPreviewHoverArmed()).toBe(true);
  });

  it("handles controls registration safely", () => {
    let hideCalled = false;
    registerCompactPreviewControls({
      forceHide: () => {
        hideCalled = true;
      },
      warmup: () => {},
      supported: () => true,
      warmupSupported: () => true,
    });

    noteCompactPreviewClipboardShown();
    expect(hideCalled).toBe(true);
  });

  it("keeps the last payload for re-sending to a preview that loaded late", () => {
    const payload = { requestId: 7, content: "hello" };
    rememberCompactPreviewPayload(7, payload);
    expect(compactPreviewPayloadFor(7)).toBe(payload);
    // A newer hover or no pending request never gets the old payload.
    expect(compactPreviewPayloadFor(8)).toBeNull();
    expect(compactPreviewPayloadFor(null)).toBeNull();

    rememberCompactPreviewPayload(8, { requestId: 8 });
    expect(compactPreviewPayloadFor(7)).toBeNull();

    forgetCompactPreviewPayload();
    expect(compactPreviewPayloadFor(8)).toBeNull();
  });

  it("sizes the fallback window within the preview bounds", () => {
    expect(compactPreviewFallbackSize({ maxWidth: 600, maxHeight: 600, minWidth: 252 }))
      .toEqual({ width: 252, height: 180 });
    expect(compactPreviewFallbackSize({ maxWidth: 600, maxHeight: 600, minWidth: 200 }))
      .toEqual({ width: 220, height: 180 });
    expect(compactPreviewFallbackSize({ maxWidth: 200, maxHeight: 150, minWidth: 240 }))
      .toEqual({ width: 200, height: 150 });
  });
});
