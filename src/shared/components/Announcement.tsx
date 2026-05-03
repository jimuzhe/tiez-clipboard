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

    // Calculate cycle interval based on current announcement text length
    const currentAnnouncement = announcements[activeIndex];
    const textLength = currentAnnouncement
        ? (currentAnnouncement.title + currentAnnouncement.message + (currentAnnouncement.linkText || '')).length
        : 0;
    // ~60px/s scroll speed, + 3s buffer after animation ends, min 13s
    const marqueeDurationMs = Math.max(10000, Math.round((textLength * 4.5) / 60) * 1000);
    const cycleInterval = marqueeDurationMs + 3000;

    useEffect(() => {
        if (announcements.length <= 1 || isPaused) {
            return;
        }

        const interval = window.setInterval(() => {
            setActiveIndex((prev) => {
                if (announcements.length === 0) return 0;
                return (prev + 1) % announcements.length;
            });
        }, cycleInterval);

        return () => {
            clearInterval(interval);
        };
    }, [announcements.length, isPaused, cycleInterval]);

    const current = announcements[activeIndex];

    if (announcements.length === 0) return null;

    return (
        <div 
            className="announcement-ticker"
            onMouseEnter={() => setIsPaused(true)}
            onMouseLeave={() => setIsPaused(false)}
        >
            <div className="ticker-icon">
                <Megaphone size={16} />
            </div>
            
            <div className="ticker-content-wrapper">
                <AnimatePresence mode="wait">
                    <motion.div
                        key={current.id}
                        initial={{ opacity: 0, x: 10 }}
                        animate={{ opacity: 1, x: 0 }}
                        exit={{ opacity: 0, x: -10 }}
                        transition={{ duration: 0.3, ease: "easeOut" }}
                        style={{ height: '100%', width: '100%' }}
                    >
                        <AnnouncementContent data={current} />
                    </motion.div>
                </AnimatePresence>
            </div>

            {announcements.length > 1 && (
                <div className="ticker-count" title={`${activeIndex + 1}/${announcements.length}`}>
                    {activeIndex + 1}/{announcements.length}
                </div>
            )}
            
            <div
                className="ticker-fixed-close"
                onClick={() => onDismiss(current.id)}
                title="Dismiss"
            >
                <X size={16} />
            </div>
        </div>
    );
};

const AnnouncementContent: React.FC<{ data: Announcement }> = ({ data }) => {
    const wrapperRef = useRef<HTMLDivElement | null>(null);
    const trackRef = useRef<HTMLDivElement | null>(null);
    const [isOverflowing, setIsOverflowing] = useState(false);

    useLayoutEffect(() => {
        const wrapper = wrapperRef.current;
        const track = trackRef.current;
        if (!wrapper || !track) return;

        const update = () => {
            const hasOverflow = track.scrollWidth > wrapper.clientWidth + 4;
            setIsOverflowing(hasOverflow);

            if (hasOverflow) {
                const scrollDist = track.scrollWidth - wrapper.clientWidth + 12;
                wrapper.style.setProperty('--scroll-dist', `-${scrollDist}px`);

                // Duration proportional to text length: ~60px/s scroll speed, min 8s
                // Scrolling occupies 70% of animation (5%-75%), so divide by 0.7*60=42
                const duration = Math.max(8, Math.round(scrollDist / 42));
                wrapper.style.setProperty('--marquee-duration', `${duration}s`);
            }
        };

        update();
        const ro = new ResizeObserver(update);
        ro.observe(wrapper);
        ro.observe(track);
        return () => ro.disconnect();
    }, [data.id, data.title, data.message]);

    return (
        <div className="ticker-content-wrapper-inner" ref={wrapperRef} style={{ height: '100%', overflow: 'hidden', display: 'flex', alignItems: 'center' }}>
            <div
                ref={trackRef}
                className={`ticker-track ticker-track-single${isOverflowing ? " ticker-track-marquee" : ""}`}
            >
                <TickerItem data={data} />
            </div>
        </div>
    );
};

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
