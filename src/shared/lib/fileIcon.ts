import { invoke } from "@tauri-apps/api/core";

const fileIconCache = new Map<string, string | null>();
const fileIconRequests = new Map<string, Promise<string | null>>();

const normalizeFileIconKey = (filePath?: string | null) => {
  const value = filePath?.trim();
  if (!value) return "";
  return value;
};

export const peekFileIcon = (filePath?: string | null) => {
  const cacheKey = normalizeFileIconKey(filePath);
  if (!cacheKey) return undefined;
  return fileIconCache.get(cacheKey);
};

export const getFileIcon = async (filePath?: string | null): Promise<string | null> => {
  const rawPath = filePath?.trim();
  const cacheKey = normalizeFileIconKey(rawPath);
  if (!rawPath || !cacheKey) return null;

  const cached = fileIconCache.get(cacheKey);
  if (cached !== undefined) {
    return cached;
  }

  const pending = fileIconRequests.get(cacheKey);
  if (pending) {
    return pending;
  }

  const request = invoke<string | null>("get_file_icon", { filePath: rawPath })
    .then((icon) => {
      const normalizedIcon = typeof icon === "string" && icon.trim() ? icon : null;
      fileIconCache.set(cacheKey, normalizedIcon);
      return normalizedIcon;
    })
    .catch((error) => {
      console.error("Failed to load file icon:", error);
      return null;
    })
    .finally(() => {
      fileIconRequests.delete(cacheKey);
    });

  fileIconRequests.set(cacheKey, request);
  return request;
};
