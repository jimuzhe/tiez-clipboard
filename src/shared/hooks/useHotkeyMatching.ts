interface MatchHotkeyOptions {
  ignoreWin?: boolean;
}

export const matchesHotkey = (
  event: KeyboardEvent,
  hotkeyStr: string,
  opts: MatchHotkeyOptions = {}
) => {
  if (!hotkeyStr) return false;

  const parts = hotkeyStr.split("+").map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return false;

  const isModifier = (p: string) => {
    const up = p.toUpperCase();
    return (
      up === "CTRL" ||
      up === "CONTROL" ||
      up === "SHIFT" ||
      up === "ALT" ||
      up === "MENU" ||
      up === "WIN" ||
      up === "SUPER" ||
      up === "COMMAND" ||
      up === "META"
    );
  };

  const ctrl = parts.some((p) => ["CTRL", "CONTROL"].includes(p.toUpperCase()));
  const shift = parts.some((p) => p.toUpperCase() === "SHIFT");
  const alt = parts.some((p) => ["ALT", "MENU"].includes(p.toUpperCase()));
  const win = parts.some((p) => ["WIN", "SUPER", "COMMAND", "META"].includes(p.toUpperCase()));

  if (event.ctrlKey !== ctrl) return false;
  if (event.shiftKey !== shift) return false;
  if (event.altKey !== alt) return false;
  if (!opts.ignoreWin) {
    if (event.metaKey !== win) return false;
  } else {
    if (!win && event.metaKey) return false;
  }

  const keyPart = parts.filter((p) => !isModifier(p)).pop();
  if (!keyPart) return false;

  const keyUpper = keyPart.toUpperCase();
  const eventKey = event.key;

  if (keyUpper === "SPACE") return eventKey === " " || eventKey === "Spacebar";
  if (keyUpper === "ENTER" || keyUpper === "RETURN") return eventKey === "Enter";
  if (keyUpper === "TAB") return eventKey === "Tab";
  if (keyUpper === "BACKSPACE") return eventKey === "Backspace";
  if (keyUpper === "DELETE") return eventKey === "Delete";
  if (keyUpper === "INSERT") return eventKey === "Insert";
  if (keyUpper === "PAGEUP") return eventKey === "PageUp";
  if (keyUpper === "PAGEDOWN") return eventKey === "PageDown";
  if (keyUpper === "END") return eventKey === "End";
  if (keyUpper === "HOME") return eventKey === "Home";
  if (keyUpper === "LEFT") return eventKey === "ArrowLeft";
  if (keyUpper === "RIGHT") return eventKey === "ArrowRight";
  if (keyUpper === "UP") return eventKey === "ArrowUp";
  if (keyUpper === "DOWN") return eventKey === "ArrowDown";
  if (keyUpper.startsWith("F") && keyUpper.length > 1) return eventKey.toUpperCase() === keyUpper;

  if (keyUpper.length === 1) {
    return eventKey.toUpperCase() === keyUpper;
  }

  return eventKey.toUpperCase() === keyUpper;
};
