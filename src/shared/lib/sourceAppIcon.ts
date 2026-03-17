import { invoke } from "@tauri-apps/api/core";

const sourceAppIconCache = new Map<string, string | null>();
const sourceAppIconRequests = new Map<string, Promise<string | null>>();

const normalizeSourceAppPath = (sourceAppPath?: string | null) => {
  const value = sourceAppPath?.trim();
  if (!value) return "";
  return value.replace(/\//g, "\\").toLowerCase();
};

export const peekSourceAppIcon = (sourceAppPath?: string | null) => {
  const cacheKey = normalizeSourceAppPath(sourceAppPath);
  if (!cacheKey) return undefined;
  return sourceAppIconCache.get(cacheKey);
};

export const getSourceAppIcon = async (sourceAppPath?: string | null): Promise<string | null> => {
  const rawPath = sourceAppPath?.trim();
  const cacheKey = normalizeSourceAppPath(rawPath);
  if (!rawPath || !cacheKey) return null;

  const cached = sourceAppIconCache.get(cacheKey);
  if (cached !== undefined) {
    return cached;
  }

  const pending = sourceAppIconRequests.get(cacheKey);
  if (pending) {
    return pending;
  }

  const request = invoke<string | null>("get_executable_icon", { executablePath: rawPath })
    .then((icon) => {
      const normalizedIcon = typeof icon === "string" && icon.trim() ? icon : null;
      sourceAppIconCache.set(cacheKey, normalizedIcon);
      return normalizedIcon;
    })
    .catch((error) => {
      console.error("Failed to load source app icon:", error);
      return null;
    })
    .finally(() => {
      sourceAppIconRequests.delete(cacheKey);
    });

  sourceAppIconRequests.set(cacheKey, request);
  return request;
};
