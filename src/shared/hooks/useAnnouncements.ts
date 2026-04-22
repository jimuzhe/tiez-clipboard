import { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { Announcement } from "../types";

export type { Announcement } from "../types";

const PING_URL = "https://tiez.name666.top/api/v1/ping";

const normalizeVersion = (val?: string | null) => {
  if (!val) return null;
  const cleaned = val.trim().replace(/^v/i, "");
  if (!cleaned) return null;
  return cleaned.split(/[+-]/)[0];
};

const parseVersion = (val?: string | null) => {
  const normalized = normalizeVersion(val);
  if (!normalized) return null;
  const parts = normalized.split(".");
  if (parts.length === 0) return null;
  const nums = parts.map((p) => Number.parseInt(p, 10));
  if (nums.some((n) => Number.isNaN(n))) return null;
  return nums;
};

const compareVersions = (a?: string | null, b?: string | null) => {
  const va = parseVersion(a);
  const vb = parseVersion(b);
  if (!va || !vb) return null;
  const len = Math.max(va.length, vb.length);
  for (let i = 0; i < len; i += 1) {
    const left = va[i] ?? 0;
    const right = vb[i] ?? 0;
    if (left > right) return 1;
    if (left < right) return -1;
  }
  return 0;
};

const matchesVersion = (announcement: Announcement, currentVersion: string | null) => {
  const hasTarget =
    !!announcement.versionTarget ||
    !!announcement.minVersion ||
    !!announcement.maxVersion;

  if (!hasTarget) return true;
  if (!currentVersion || currentVersion === "unknown") return false;

  if (announcement.versionTarget) {
    const eq = compareVersions(currentVersion, announcement.versionTarget);
    return eq === 0;
  }

  if (announcement.minVersion) {
    const cmp = compareVersions(currentVersion, announcement.minVersion);
    if (cmp === null || cmp < 0) return false;
  }

  if (announcement.maxVersion) {
    const cmp = compareVersions(currentVersion, announcement.maxVersion);
    if (cmp === null || cmp > 0) return false;
  }

  return true;
};

export function useAnnouncements() {
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchAnnouncements = async () => {
      try {
        // Get device ID (optional, using stored random ID or generating one)
        let deviceId = localStorage.getItem("device_id");
        if (!deviceId) {
          deviceId = Math.random().toString(36).substring(7);
          localStorage.setItem("device_id", deviceId);
        }

        // Get real version dynamically
        let version = "unknown";
        try {
          version = await getVersion();
        } catch (err) {
          console.warn("Failed to get version:", err);
        }

        // Construct URL with query params
        const url = `${PING_URL}?id=${deviceId}&v=${version}`;

        const response = await fetch(url, {
          signal: AbortSignal.timeout(3000),
        });

        if (!response.ok) throw new Error("Ping failed");

        const data = await response.json();
        const fetchedBroadcasts = data.broadcasts || [];

        // Filter out dismissed announcements
        const dismissed = JSON.parse(
          localStorage.getItem("dismissed_announcements") || "[]"
        );
        const versioned = fetchedBroadcasts.filter((a: Announcement) =>
          matchesVersion(a, version)
        );
        const validAnnouncements = versioned.filter(
          (a: Announcement) => !dismissed.includes(a.id)
        );

        setAnnouncements(validAnnouncements);
      } catch (error) {
        console.warn("Failed to fetch announcements/ping.", error);
        // We don't clear existing announcements on polling failure to avoid flickering
      } finally {
        setLoading(false);
      }
    };

    // 1. Initial fetch
    fetchAnnouncements();

    // 2. Setup periodic polling (every 6 hours)
    const POLLING_INTERVAL = 6 * 60 * 60 * 1000;
    const interval = setInterval(fetchAnnouncements, POLLING_INTERVAL);

    return () => {
      clearInterval(interval);
    };
  }, []);

  const dismissAnnouncement = (id: string, forever: boolean = true) => {
    setAnnouncements((prev) => prev.filter((a) => a.id !== id));
    if (forever) {
      const dismissed = JSON.parse(
        localStorage.getItem("dismissed_announcements") || "[]"
      );
      if (!dismissed.includes(id)) {
        dismissed.push(id);
        localStorage.setItem(
          "dismissed_announcements",
          JSON.stringify(dismissed)
        );
      }
    }
  };

  return { announcements, loading, dismissAnnouncement };
}
