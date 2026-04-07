export type HotkeyDisplayToken = {
  raw: string;
  label: string;
  isSymbol: boolean;
};

const MAC_SYMBOL_LABELS: Record<string, string> = {
  COMMAND: "⌘",
  CMD: "⌘",
  META: "⌘",
  WIN: "⌘",
  SUPER: "⌘",
  OPTION: "⌥",
  ALT: "⌥",
  SHIFT: "⇧",
  CTRL: "⌃",
  CONTROL: "⌃",
  ENTER: "↩",
  RETURN: "↩",
  TAB: "⇥",
  SPACE: "␣",
  SPACEBAR: "␣",
  BACKSPACE: "⌫",
  DELETE: "⌦",
  ESC: "⎋",
  ESCAPE: "⎋",
  UP: "↑",
  ARROWUP: "↑",
  DOWN: "↓",
  ARROWDOWN: "↓",
  LEFT: "←",
  ARROWLEFT: "←",
  RIGHT: "→",
  ARROWRIGHT: "→",
  HOME: "↖",
  END: "↘",
  PAGEUP: "⇞",
  PAGEDOWN: "⇟"
};

const PLAIN_LABELS: Record<string, string> = {
  COMMAND: "Command",
  CMD: "Command",
  META: "Meta",
  WIN: "Win",
  SUPER: "Super",
  OPTION: "Option",
  ALT: "Alt",
  SHIFT: "Shift",
  CTRL: "Ctrl",
  CONTROL: "Ctrl",
  ESCAPE: "Esc",
  RETURN: "Enter",
  SPACEBAR: "Space",
  ARROWUP: "Up",
  ARROWDOWN: "Down",
  ARROWLEFT: "Left",
  ARROWRIGHT: "Right",
  PAGEUP: "PgUp",
  PAGEDOWN: "PgDn"
};

const detectMacPlatform = (): boolean => {
  if (typeof navigator === "undefined") return false;
  return (
    /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent) ||
    /Mac/i.test(navigator.platform)
  );
};

const normalizePart = (raw: string): string => raw.trim();

const toToken = (part: string, isMac: boolean): HotkeyDisplayToken => {
  const raw = normalizePart(part);
  const key = raw.toUpperCase();

  if (isMac) {
    const symbol = MAC_SYMBOL_LABELS[key];
    if (symbol) return { raw, label: symbol, isSymbol: true };
  }

  const plain = PLAIN_LABELS[key];
  if (plain) return { raw, label: plain, isSymbol: false };
  return { raw, label: key, isSymbol: false };
};

export const getHotkeyDisplayTokens = (
  hotkey: string | undefined,
  opts: { preferMacSymbols?: boolean } = {}
): HotkeyDisplayToken[] => {
  const value = (hotkey || "").trim();
  if (!value) return [];

  const isMac = opts.preferMacSymbols ?? detectMacPlatform();
  return value
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => toToken(part, isMac));
};
