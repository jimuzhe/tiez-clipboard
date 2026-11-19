// Tells a pointer that is scanning a list from one that is crossing it on the
// way to the pane beside it (the "menu aim" idea). Scanning should select rows
// at once; crossing must not retarget the pane it is heading for.

interface PointerAimOptions {
  /** Travel (px) before the direction is judged again; filters hand jitter. */
  minTravel?: number;
  /** Sideways share of the move that counts as heading for the pane: 0.35 is within about 70° of horizontal. */
  minSideways?: number;
}

export interface PointerAim {
  /** Records a pointer position; true while the pointer is heading right, toward the pane. */
  note: (x: number, y: number) => boolean;
  reset: () => void;
}

export const createPointerAim = ({ minTravel = 6, minSideways = 0.35 }: PointerAimOptions = {}): PointerAim => {
  let anchor: { x: number; y: number } | null = null;
  let towardPane = false;
  return {
    note: (x, y) => {
      if (!anchor) {
        anchor = { x, y };
        return towardPane;
      }
      const dx = x - anchor.x;
      const dy = y - anchor.y;
      if (Math.hypot(dx, dy) >= minTravel) {
        towardPane = dx > 0 && dx >= Math.abs(dy) * minSideways;
        anchor = { x, y };
      }
      return towardPane;
    },
    reset: () => {
      anchor = null;
      towardPane = false;
    },
  };
};
