import { beforeEach, expect, test, vi } from "vitest";

const handlers = new Map<string, () => void>();
vi.mock("react", () => ({ useEffect: (effect: () => void) => effect() }));
vi.mock("../../../shared/lib/tauriRuntime", () => ({ isTauriRuntime: () => true }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: () => void) => {
    handlers.set(name, handler);
    return Promise.resolve(() => handlers.delete(name));
  },
}));

const { useRevealReset } = await import("./useRevealReset");

let revealed = new Set<number>();
const setRevealedIds = (next: Set<number> | ((prev: Set<number>) => Set<number>)) => {
  revealed = typeof next === "function" ? next(revealed) : next;
};

beforeEach(() => {
  handlers.clear();
  revealed = new Set([3, 7]);
  useRevealReset(setRevealedIds);
});

test.each(["clipboard-hidden", "clipboard-shown"])("%s hides every revealed entry again", (event) => {
  handlers.get(event)?.();
  expect(revealed.size).toBe(0);
});

test("nothing revealed keeps the same set, so the list does not re-render", () => {
  revealed = new Set();
  const before = revealed;
  handlers.get("clipboard-hidden")?.();
  expect(revealed).toBe(before);
});
