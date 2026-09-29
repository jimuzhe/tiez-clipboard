import { useCallback, useMemo } from "react";
import { VirtualClipboardList } from "./VirtualClipboardList";
import ClipboardItem from "./ClipboardItem";
import type { ClipboardEntry } from "../../../shared/types";

const ITEM_COUNT = 400;

const buildItems = (): ClipboardEntry[] => {
  const items: ClipboardEntry[] = [];
  for (let i = 0; i < ITEM_COUNT; i += 1) {
    const kind = i % 7;
    const contentType = kind === 0 ? "image" : kind === 1 ? "file" : kind === 2 ? "rich_text" : "text";
    const body = `剪贴板记录 ${i} `.repeat(18);
    items.push({
      id: i + 1,
      content_type: contentType,
      content: contentType === "image"
        ? "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
        : contentType === "file"
          ? `/tmp/tiez-bench-${i}.txt`
          : body,
      html_content: contentType === "rich_text" ? `<p>${body}</p>` : undefined,
      source_app: i % 2 === 0 ? "Safari" : "Notes",
      timestamp: 1_700_000_000_000 - i * 60_000,
      preview: body.slice(0, 180),
      is_pinned: false,
      tags: i % 5 === 0 ? ["工作"] : [],
    });
  }
  return items;
};

const noop = () => {};
const noopEvent = () => {};

/**
 * Isolated list used by scripts/scroll-blank-repro.mjs.
 * Renders the real shell + virtual clipboard list so fast-scroll blanks can be measured.
 */
export function ScrollBench() {
  const items = useMemo(() => buildItems(), []);

  const renderItem = useCallback((item: ClipboardEntry) => (
    <ClipboardItem
      item={item}
      isSelected={false}
      windowPinned={false}
      isSensitiveHidden={false}
      isRevealed={false}
      isEditingTags={false}
      tagInput=""
      theme="sakura"
      language="zh"
      t={(key) => (key === "open" ? "打开" : key)}
      compactMode={false}
      showSourceAppIcon={false}
      aiEnabled={false}
      onSelect={noop}
      onCopy={noop}
      onToggleReveal={noopEvent}
      onOpen={noopEvent}
      onTogglePin={noopEvent}
      onDelete={noopEvent}
      onToggleTagEditor={noopEvent}
      onTagInput={noop}
      onTagAdd={noop}
      onTagDelete={noop}
      disableLayout
    />
  ), []);

  return (
    <div className="app-container" data-scroll-bench="ready" style={{ height: "100%" }}>
      <div className="main-content" style={{ height: "100%" }}>
        <VirtualClipboardList
          items={items}
          renderItem={renderItem}
          hasMore={false}
          isLoading={false}
          selectedIndex={-1}
          isKeyboardMode={false}
          compactMode={false}
        />
      </div>
    </div>
  );
}
