import { convertFileSrc } from "@tauri-apps/api/core";

const decodeUriComponentSafely = (value: string, rounds = 2): string => {
  let out = value;
  for (let i = 0; i < rounds; i++) {
    if (!/%[0-9a-fA-F]{2}/.test(out)) break;
    try {
      const decoded = decodeURIComponent(out);
      if (decoded === out) break;
      out = decoded;
    } catch {
      break;
    }
  }
  return out;
};

export const toTauriLocalImageSrc = (rawSrc: string): string | null => {
  let value = (rawSrc || "").trim();
  if (!value) return null;

  // Non-local sources should not be converted.
  if (/^(data:|https?:|blob:|asset:|tauri:)/i.test(value)) {
    return null;
  }

  value = decodeUriComponentSafely(value, 2);

  if (/^file:/i.test(value)) {
    value = value.replace(/^file:\/+/i, "");
    if (/^\/[a-zA-Z]:/.test(value)) {
      value = value.slice(1);
    }
  }

  if (!/^[a-zA-Z]:[\\/]/.test(value)) {
    return null;
  }

  return convertFileSrc(value);
};

