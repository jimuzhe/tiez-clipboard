// Turns a cloud-sync failure into something the user can act on. The backend
// reports what it was doing ("webdav MOVE publish failed for a -> b: HTTP 401
// Unauthorized"); the user needs to know which setting to fix, or that nothing
// needs fixing. The HTTP status stays in brackets for bug reports.

import { failureText, formatError } from "./errorMessage";

type Translate = (key: string) => string;

const STATUS_RE = /\b(?:HTTP|status)\s+(\d{3})\b/i;
const UNREACHABLE_RE = /error sending request|dns error|connection refused|connection reset|timed out|tcp connect|failed to lookup|certificate/i;

const STATUS_KEYS: Record<string, string> = {
  "401": "sync_error_auth",
  "403": "sync_error_auth",
  "404": "sync_error_address",
  "405": "sync_error_address",
  "429": "sync_error_busy",
  "502": "sync_error_busy",
  "503": "sync_error_busy",
  "504": "sync_error_busy",
  "507": "sync_error_full",
};

/** Text for the "last error" line and the "sync now" toast. */
export const syncErrorText = (err: unknown, t: Translate): string => {
  const raw = formatError(err, t);
  // Unrecognised: the message without its backend tag. A message that is only
  // a tag ("[network]") would leave nothing, so then the translated kind.
  const asIs = () => failureText("", err) || raw.replace(/\s*[:：]\s*$/, "");
  const status = STATUS_RE.exec(raw)?.[1];
  const key = status ? STATUS_KEYS[status] : UNREACHABLE_RE.test(raw) ? "sync_error_unreachable" : undefined;
  if (!key) return asIs();
  const text = t(key);
  if (text === key) return asIs();
  return status ? `${text} (HTTP ${status})` : text;
};
