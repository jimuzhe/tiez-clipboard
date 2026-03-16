import type { Locale } from "../types";

const SENSITIVE_MASK = "...";
const URL_PROTOCOL_RE = /^(https?:\/\/)/i;

const maskMiddleChars = (value: string, prefixVisibleCount: number, suffixVisibleCount: number) => {
  const chars = Array.from(value);
  if (chars.length <= prefixVisibleCount + suffixVisibleCount) {
    return value;
  }

  const prefix = chars.slice(0, prefixVisibleCount).join("");
  const suffix = chars.slice(chars.length - suffixVisibleCount).join("");
  return `${prefix}${SENSITIVE_MASK}${suffix}`;
};

// Helper function to generate a consistent color from a string based on theme
export const getTagColor = (tag: string, theme: string) => {
  let hash = 0;
  for (let i = 0; i < tag.length; i++) {
    hash = tag.charCodeAt(i) + ((hash << 5) - hash);
  }

  // Use a larger prime and multiple rotations to ensure strings that are similar
  // (like "tag1" and "tag2") produce very different hues.
  const hue = Math.abs((hash * 137.508 + (hash >> 3)) % 360);

  if (theme === "retro") {
    // Retro: Slightly desaturated, lower lightness for mechanical look
    return `hsl(${hue}, 60%, 40%)`;
  } else {
    // Modern: Vibrant for Mica/Acrylic
    return `hsl(${hue}, 80%, 55%)`;
  }
};

export const getConciseTime = (timestamp: number, language: Locale) => {
  const diff = Date.now() - timestamp;
  const seconds = Math.floor(diff / 1000);

  if (language === "zh") {
    if (seconds < 60) return "< 1分钟";
    if (seconds < 3600) return `${Math.floor(seconds / 60)}分钟前`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}小时前`;
    return `${Math.floor(seconds / 86400)}天前`;
  } else {
    if (seconds < 60) return "< 1m";
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
    return `${Math.floor(seconds / 86400)}d ago`;
  }
};

export const formatSensitivePreview = (content: string, contentType: string) => {
  if (!content) return "";

  if (contentType === "url") {
    const protocolMatch = content.match(URL_PROTOCOL_RE);
    if (!protocolMatch) {
      return maskMiddleChars(content, 6, 4);
    }

    const protocol = protocolMatch[0];
    const rest = content.slice(protocol.length);
    const maskedRest = maskMiddleChars(rest, 6, 4);
    return maskedRest === rest ? content : `${protocol}${maskedRest}`;
  }

  return maskMiddleChars(content, 3, 3);
};
