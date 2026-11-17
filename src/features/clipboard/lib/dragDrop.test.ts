import { test, expect } from "vitest";
import { hiddenDragLabel, nativeDragPayload, shouldHideAfterExternalDrop } from "./dragDrop";
import type { ClipboardEntry } from "../../../shared/types";

test("accepted external drops hide an unpinned window", () => {
  expect(shouldHideAfterExternalDrop("copy", false, false)).toBe(true);
  expect(shouldHideAfterExternalDrop("move", false, false)).toBe(true);
});

test("Escape, rejected drops, internal drops and pinned windows stay visible", () => {
  expect(shouldHideAfterExternalDrop("none", false, false)).toBe(false);
  expect(shouldHideAfterExternalDrop("copy", true, false)).toBe(false);
  expect(shouldHideAfterExternalDrop("copy", false, true)).toBe(false);
});

const entry = (content_type: string, content: string, html_content?: string) =>
  ({ id: 1, content_type, content, html_content, source_app: "", timestamp: 0, is_pinned: false, tags: [], use_count: 0 }) as unknown as ClipboardEntry;

test("a native drag hands over image, video and file entries as files", () => {
  expect(nativeDragPayload(entry("image", "/data/attachments/img_1.png"), [])?.files).toEqual(["/data/attachments/img_1.png"]);
  expect(nativeDragPayload(entry("video", "/Movies/clip.mov"), [])?.files).toEqual(["/Movies/clip.mov"]);
  expect(nativeDragPayload(entry("file", "/a.md\n/b.pdf"), [])?.files).toEqual(["/a.md", "/b.pdf"]);
  expect(nativeDragPayload(entry("file", "ignored"), ["/c.txt"])?.files).toEqual(["/c.txt"]);
});

test("an embedded image has no file, so it keeps the web drag", () => {
  expect(nativeDragPayload(entry("image", "data:image/png;base64,AAAA"), [])).toBeNull();
});

test("text entries carry their text, plus HTML or a link where they have one", () => {
  expect(nativeDragPayload(entry("text", "hello"), [])).toEqual({ files: [], text: "hello", html: null, url: null });
  expect(nativeDragPayload(entry("url", " https://a.example "), [])).toEqual({ files: [], text: "https://a.example", html: null, url: "https://a.example" });
  expect(nativeDragPayload(entry("code", "a < b"), [])?.html).toBe("<pre><code>a &lt; b</code></pre>");
  expect(nativeDragPayload(entry("rich_text", "Bold", "<b>Bold</b>"), [])?.html).toBe("<b>Bold</b>");
  expect(nativeDragPayload(entry("text", ""), [])).toBeNull();
});

test("a hidden sensitive entry drags under its masked label, never its content", () => {
  const t = (key: string) => `t:${key}`;
  const entry = (content_type: string, content: string) =>
    ({ id: 1, content, content_type, source_app: "", timestamp: 0, preview: content, is_pinned: false }) as ClipboardEntry;

  expect(hiddenDragLabel(entry("text", "hunter2-secret"), "hun*******ret", t)).toBe("hun*******ret");
  expect(hiddenDragLabel(entry("text", "x"), "", t)).toBe("SENSITIVE");
  expect(hiddenDragLabel(entry("file", "/Users/me/Taxes/2025.pdf"), "/Us***pdf", t)).toBe("SENSITIVE");
  expect(hiddenDragLabel(entry("video", "/Users/me/private.mov"), "/Us***mov", t)).toBe("SENSITIVE");
  expect(hiddenDragLabel(entry("image", "/tmp/a.png"), "/tm***png", t)).toBe("t:image");
});
