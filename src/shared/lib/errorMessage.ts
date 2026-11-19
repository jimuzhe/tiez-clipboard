// Turns whatever a failed call produced into short, readable UI text.
//
// Backend errors arrive as "[kind] detail" (see src-tauri/src/error.rs); the
// kind is translated here. Anything else (third-party text, HTML error pages,
// stack traces) is stripped of markup and shortened so it can never flood a toast.

type Translate = (key: string) => string;

const KIND_KEYS: Record<string, string> = {
  db: "error_kind_db",
  io: "error_kind_io",
  network: "error_kind_network",
  internal: "error_kind_internal",
  validation: "error_kind_validation",
  encryption: "error_kind_encryption",
};

// Builds before the tagged format used fixed Chinese prefixes.
const LEGACY_PREFIXES: Record<string, string> = {
  数据库错误: "db",
  文件系统错误: "io",
  网络错误: "network",
  内部系统错误: "internal",
  验证错误: "validation",
  加密错误: "encryption",
};

const TAG_RE = /\[(db|io|network|internal|validation|encryption)\]\s*/g;
const LEGACY_RE = new RegExp(`(${Object.keys(LEGACY_PREFIXES).join("|")}):\\s*`, "g");

export const MAX_ERROR_CHARS = 200;

export const errorToText = (err: unknown): string => {
  if (err == null) return "";
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (typeof err === "object") {
    const message = (err as { message?: unknown }).message;
    if (typeof message === "string") return message;
    try {
      return JSON.stringify(err);
    } catch {
      return String(err);
    }
  }
  return String(err);
};

/** Strip markup, collapse whitespace and cap the length. */
export const tidyMessage = (text: string, max = MAX_ERROR_CHARS): string => {
  const plain = text
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<(script|style)[\s\S]*?<\/\1>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return plain.length > max ? `${plain.slice(0, max - 1).trimEnd()}…` : plain;
};

/** Replace backend error tags anywhere in a message with translated labels. */
export const localizeErrorTags = (text: string, t?: Translate): string => {
  const label = (kind: string) => {
    const key = KIND_KEYS[kind];
    const translated = key && t ? t(key) : "";
    return translated && translated !== key ? `${translated}: ` : "";
  };
  return text
    .replace(TAG_RE, (_m, kind: string) => label(kind))
    .replace(LEGACY_RE, (_m, prefix: string) => label(LEGACY_PREFIXES[prefix]));
};

/** Text for a toast or status line describing `err`. */
export const formatError = (err: unknown, t?: Translate, max = MAX_ERROR_CHARS): string =>
  tidyMessage(localizeErrorTags(errorToText(err), t), max);

/** Remove backend error tags: next to a specific label the error kind adds nothing. */
export const stripErrorTags = (text: string): string => text.replace(TAG_RE, "").replace(LEGACY_RE, "");

/**
 * Toast text for a failed action: "<what failed>：<why>".
 * `label` is the translated "X failed" text, with or without its own closing
 * punctuation. A detail that already opens with the label is shown alone, and
 * a missing detail leaves just the label.
 */
export const failureText = (label: string, err: unknown, max = MAX_ERROR_CHARS): string => {
  const what = label.trim().replace(/[:：。.\s]+$/u, "");
  const why = tidyMessage(stripErrorTags(errorToText(err)), max);
  if (!why) return label.trim().replace(/[:：\s]+$/u, "");
  if (!what || why.startsWith(what)) return why;
  const joiner = /[\u3400-\u9fff]/.test(what) ? "：" : ": ";
  return `${what}${joiner}${why}`;
};
