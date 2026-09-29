import React, { useRef, useImperativeHandle, useCallback, useMemo, useEffect } from 'react';
import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';
import type { ListRange, ScrollerProps } from 'react-virtuoso';
import type { ClipboardEntry } from "../../../shared/types";
import type { VirtualClipboardListHandle, VirtualClipboardListProps } from "../types";
import { isMacPlatform } from "../../../shared/lib/platform";

type VirtuosoListContext = {
    header?: React.ReactNode;
    hasMore: boolean;
    isLoading: boolean;
};

type MacScrollerProps = ScrollerProps & { context?: unknown };

const MAC_LIST = isMacPlatform();
const FAST_PX_PER_MS = 1.1;
const LOAD_MORE_REMAINING = 12;

const ListHeader = ({ context }: { context?: VirtuosoListContext }) => {
    const header = context?.header;
    return header ? <div className="list-header">{header}</div> : null;
};

const ListFooter = ({ context }: { context?: VirtuosoListContext }) => {
    if (!context) return null;
    const { isLoading, hasMore } = context;
    if (!isLoading && !hasMore) return null;

    return (
        <div style={{
            padding: '20px',
            textAlign: 'center',
            opacity: 0.6,
            fontSize: '12px',
            color: 'var(--text-secondary)'
        }}>
            {isLoading ? '加载中...' : '加载更多...'}
        </div>
    );
};

const MacScroller = React.forwardRef<HTMLDivElement, MacScrollerProps>(function MacScroller(props, ref) {
    const { style, context, ...rest } = props;
    void context;
    return (
        <div
            ref={ref}
            {...rest}
            data-mac-scroller="auto"
            style={{
                ...style,
                WebkitOverflowScrolling: "auto",
                overflowX: "hidden",
            }}
        />
    );
});

const VirtualClipboardList = React.forwardRef<VirtualClipboardListHandle, VirtualClipboardListProps>(
    (props, ref) => {
        const {
            items,
            renderItem,
            onLoadMore,
            hasMore,
            isLoading,
            selectedIndex,
            isKeyboardMode,
            onScroll,
            compactMode,
            header
        } = props;

        const virtuosoRef = useRef<VirtuosoHandle>(null);
        const visibleRangeRef = useRef<ListRange | null>(null);
        const scrollerNodeRef = useRef<HTMLElement | null>(null);
        const repaintLockRef = useRef(false);
        const settleTimerRef = useRef<number | null>(null);
        const lastRepaintRef = useRef(0);
        const velocityRef = useRef({ top: 0, time: 0, fast: false });
        const nearEndRef = useRef(false);

        const nudgeRepaint = useCallback(() => {
            const el = scrollerNodeRef.current;
            if (!MAC_LIST || !el || repaintLockRef.current) return;
            if (performance.now() - lastRepaintRef.current < 400) return;
            const max = Math.max(0, el.scrollHeight - el.clientHeight);
            if (max <= 0) return;
            lastRepaintRef.current = performance.now();
            repaintLockRef.current = true;
            const top = el.scrollTop;
            const nudged = top <= 0 ? Math.min(1, max) : top - 1;
            el.scrollTop = nudged;
            requestAnimationFrame(() => {
                el.scrollTop = top;
                requestAnimationFrame(() => {
                    repaintLockRef.current = false;
                });
            });
        }, []);

        const loadMoreIfNeeded = useCallback(() => {
            if (!nearEndRef.current || velocityRef.current.fast) return;
            if (hasMore && !isLoading && onLoadMore) onLoadMore();
        }, [hasMore, isLoading, onLoadMore]);

        useEffect(() => {
            return () => {
                if (settleTimerRef.current !== null) window.clearTimeout(settleTimerRef.current);
            };
        }, []);
        useImperativeHandle(ref, () => ({
            scrollToItem: (index: number) => {
                virtuosoRef.current?.scrollIntoView({
                    index,
                    behavior: 'smooth',
                    align: 'center',
                });
            },
            scrollToTop: () => {
                virtuosoRef.current?.scrollTo({
                    top: 0,
                    behavior: 'auto'
                });
            },
            resetAfterIndex: (_index: number) => {
                // Not needed with Virtuoso as it handles dynamic heights automatically
            }
        }));

        // Keep keyboard selection visible even when the item is only in overscan
        React.useEffect(() => {
            if (!isKeyboardMode || selectedIndex < 0) return;

            const range = visibleRangeRef.current;
            const edgeBuffer = 1;

            if (!range) {
                virtuosoRef.current?.scrollToIndex({
                    index: selectedIndex,
                    behavior: 'auto',
                    align: 'center',
                });
                return;
            }

            if (selectedIndex < range.startIndex + edgeBuffer) {
                virtuosoRef.current?.scrollToIndex({
                    index: selectedIndex,
                    behavior: 'auto',
                    align: 'start',
                });
                return;
            }

            if (selectedIndex > range.endIndex - edgeBuffer) {
                virtuosoRef.current?.scrollToIndex({
                    index: selectedIndex,
                    behavior: 'auto',
                    align: 'end',
                });
            }
        }, [selectedIndex, isKeyboardMode]);

        const handleScroll = useCallback((scrollTop: number) => {
            onScroll?.(scrollTop);
            if (!MAC_LIST || repaintLockRef.current) return;
            const now = performance.now();
            const prev = velocityRef.current;
            const dt = now - prev.time;
            if (prev.time > 0 && dt >= 16 && Math.abs(scrollTop - prev.top) / dt >= FAST_PX_PER_MS) {
                prev.fast = true;
            }
            prev.top = scrollTop;
            prev.time = now;
            if (settleTimerRef.current !== null) window.clearTimeout(settleTimerRef.current);
            settleTimerRef.current = window.setTimeout(() => {
                settleTimerRef.current = null;
                const wasFast = velocityRef.current.fast;
                velocityRef.current.fast = false;
                if (wasFast) nudgeRepaint();
                loadMoreIfNeeded();
            }, 120);
        }, [loadMoreIfNeeded, nudgeRepaint, onScroll]);

        const handleEndReached = useCallback(() => {
            nearEndRef.current = true;
            if (velocityRef.current.fast) return;
            if (hasMore && !isLoading && onLoadMore) onLoadMore();
        }, [hasMore, isLoading, onLoadMore]);

        const handleRangeChanged = useCallback((range: ListRange) => {
            visibleRangeRef.current = range;
            nearEndRef.current = items.length > 0 && items.length - range.endIndex <= LOAD_MORE_REMAINING;
            if (nearEndRef.current && !velocityRef.current.fast) {
                if (hasMore && !isLoading && onLoadMore) onLoadMore();
            }
        }, [hasMore, isLoading, items.length, onLoadMore]);

        const itemContent = useCallback((index: number, item: ClipboardEntry) => {
            return (
                <div style={{ paddingBottom: compactMode ? 2 : 4 }}>
                    {renderItem(item, index, index === 0)}
                </div>
            );
        }, [renderItem, compactMode]);

        const components = useMemo(() => ({
            Header: ListHeader,
            Footer: ListFooter,
            ...(MAC_LIST ? { Scroller: MacScroller } : {}),
        }), []);

        const context = useMemo(() => ({
            header,
            hasMore,
            isLoading
        }), [header, hasMore, isLoading]);

        return (
            <div className="virtual-list-wrapper" style={{ height: '100%', width: '100%' }}>
                <Virtuoso
                    ref={virtuosoRef}
                    data={items}
                    computeItemKey={(_index, item) => item.id}
                    itemContent={itemContent}
                    components={components}
                    context={context}
                    style={{ height: '100%' }}
                    scrollerRef={(node) => {
                        scrollerNodeRef.current = node instanceof HTMLElement ? node : null;
                    }}
                    onScroll={(e) => handleScroll((e.currentTarget as HTMLElement).scrollTop)}
                    endReached={handleEndReached}
                    rangeChanged={handleRangeChanged}
                    defaultItemHeight={compactMode ? 52 : 108}
                    overscan={MAC_LIST ? { main: 280, reverse: 200 } : 200}
                    increaseViewportBy={MAC_LIST ? { top: 240, bottom: 420 } : undefined}
                />
            </div>
        );
    }
);

VirtualClipboardList.displayName = 'VirtualClipboardList';

export { VirtualClipboardList };
export default VirtualClipboardList;
