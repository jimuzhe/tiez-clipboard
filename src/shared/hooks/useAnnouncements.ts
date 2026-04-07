import { useState, useEffect, useCallback, useRef } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { isTauriRuntime } from "../lib/tauriRuntime";
import type { Announcement } from "../types/announcement";

export function useAnnouncements() {
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);
  const [loading, setLoading] = useState(true);
  const timerRef = useRef<NodeJS.Timeout | null>(null);

  const fetchAnnouncements = useCallback(async () => {
    if (!isTauriRuntime()) {
      setLoading(false);
      return;
    }

    try {
      const pingUrl =
        import.meta.env.VITE_TIEZ_ANNOUNCEMENT_PING_URL ||
        import.meta.env.VITE_PING_URL ||
        `${import.meta.env.VITE_API_BASE_URL || "https://tiez.name666.top"}/api/v1/ping`;
      
      let deviceId = localStorage.getItem("device_id");
      if (!deviceId) {
        deviceId = Math.random().toString(36).substring(7);
        localStorage.setItem("device_id", deviceId);
      }

      const version = await getVersion();
      const url = `${pingUrl}?id=${deviceId}&v=${version}`;

      const response = await fetch(url, {
        signal: AbortSignal.timeout(5000),
      });

      if (!response.ok) throw new Error("Ping failed");

      const data = await response.json();
      const fetchedBroadcasts = data.broadcasts || [];

      // Filter out dismissed ones
      const dismissed = JSON.parse(
        localStorage.getItem("dismissed_announcements") || "[]"
      );
      
      const validAnnouncements = fetchedBroadcasts.filter(
        (a: Announcement) => !dismissed.includes(a.id)
      );

      setAnnouncements(validAnnouncements);
    } catch (error) {
      console.warn("[Announcements] Fetch failed:", error);
      setAnnouncements([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAnnouncements();

    const CHECK_INTERVAL = 6 * 60 * 60 * 1000;
    timerRef.current = setInterval(fetchAnnouncements, CHECK_INTERVAL);

    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [fetchAnnouncements]);

  const dismissAnnouncement = (id: string) => {
    setAnnouncements((prev) => prev.filter((a) => a.id !== id));
    const dismissed = JSON.parse(
      localStorage.getItem("dismissed_announcements") || "[]"
    );
    if (!dismissed.includes(id)) {
      dismissed.push(id);
      localStorage.setItem("dismissed_announcements", JSON.stringify(dismissed));
    }
  };

  return { announcements, loading, dismissAnnouncement, refetch: fetchAnnouncements };
}
