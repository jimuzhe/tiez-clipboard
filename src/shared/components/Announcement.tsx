import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { X, Megaphone } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import type { Announcement } from "../types";

interface AnnouncementProps {
    announcements: Announcement[];
    onDismiss: (id: string, forever?: boolean) => void;
}

export const AnnouncementSystem: React.FC<AnnouncementProps> = ({ announcements, onDismiss }) => {
    const [activeIndex, setActiveIndex] = useState(0);
    const [isPaused, setIsPaused] = useState(false);

    useEffect(() => {
        if (announcements.length === 0) {
            setActiveIndex(0);
            return;
        }
        if (activeIndex >= announcements.length) {
            setActiveIndex(Math.max(0, announcements.length - 1));
        }
    }, [announcements, activeIndex]);

    useEffect(() => {
        if (announcements.length <= 1 || isPaused) {
            return;
        }
        const interval = window.setInterval(() => {
            setActiveIndex((prev) => {
                if (announcements.length === 0) return 0;
                return (prev + 1) % announcements.length;
            });
        }, 6000);
        return () => {
            clearInterval(interval);
        };
    }, [announcements.length, isPaused]);

    const current = announcements[activeIndex];

    return (
        <>
            <AnimatePresence>
                {current && (
                    <AnnouncementTicker
                        data={current}
                        index={activeIndex}
                        total={announcements.length}
                        onDismiss={onDismiss}
                        onPauseChange={setIsPaused}
                    />
                )}
            </AnimatePresence>
        </>
    );
};

const AnnouncementTicker: React.FC<{
    data: Announcement;
    index: number;
    total: number;
    onDismiss: (id: string) => void;
    onPauseChange?: (paused: boolean) => void;
}> = ({ data, index, total, onDismiss, onPauseChange }) => {
    const wrapperRef = useRef<HTMLDivElement | null>(null);
    const trackRef = useRef<HTMLDivElement | null>(null);
    const [isOverflowing, setIsOverflowing] = useState(false);

    useLayoutEffect(() => {
        const wrapper = wrapperRef.current;
        const track = trackRef.current;
        if (!wrapper || !track) return;

        const update = () => {
            const overflow = track.scrollWidth > wrapper.clientWidth + 8;
            setIsOverflowing(overflow);
        };

        update();
        const ro = new ResizeObserver(update);
        ro.observe(wrapper);
        ro.observe(track);
        return () => ro.disconnect();
    }, [data.id, data.title, data.message, data.link, data.linkText, data.type]);

    return (
        <motion.div
            className="announcement-ticker"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 36, opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            onMouseEnter={() => onPauseChange?.(true)}
            onMouseLeave={() => onPauseChange?.(false)}
        >
            <div className="ticker-icon">
                <Megaphone size={16} />
            </div>
            <div className="ticker-content-wrapper" ref={wrapperRef}>
                <div
                    ref={trackRef}
                    className={`ticker-track ticker-track-single${isOverflowing ? " ticker-track-marquee" : ""}`}
                >
                    <TickerItem data={data} />
                </div>
            </div>
            {total > 1 && (
                <div className="ticker-count" title={`${index + 1}/${total}`}>
                    {index + 1}/{total}
                </div>
            )}
            {/* Fixed Close Button ONLY on the right */}
            <div
                className="ticker-fixed-close"
                onClick={() => onDismiss(data.id)}
                title="Dismiss"
            >
                <X size={16} />
            </div>
        </motion.div>
    );
}

const TickerItem: React.FC<{ data: Announcement }> = ({ data }) => {
    const textStyle = data.textColor ? { color: data.textColor } : undefined;
    return (
        <div className="ticker-item">
            <span className={`ticker-tag ${data.type}`}>{data.type}</span>
            <span className="ticker-text" style={textStyle}>
                <strong>{data.title}:</strong> {data.message}
            </span>
            {data.link && (
                <span
                    className="ticker-link"
                    style={textStyle}
                    onClick={(e) => {
                        e.stopPropagation();
                        invoke("open_content", { id: 0, content: data.link!, contentType: 'url' }).catch(console.error);
                    }}
                >
                    [{data.linkText || 'LINK'}]
                </span>
            )}
        </div>
    )
}
