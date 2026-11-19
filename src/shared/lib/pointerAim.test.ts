import { describe, expect, test } from "vitest";
import { createPointerAim } from "./pointerAim";

describe("createPointerAim", () => {
  test("moving up and down the list is not heading for the pane", () => {
    const aim = createPointerAim();
    aim.note(100, 100);
    expect(aim.note(101, 130)).toBe(false);
    expect(aim.note(99, 90)).toBe(false);
  });

  test("moving right, or diagonally right, is heading for the pane", () => {
    const aim = createPointerAim();
    aim.note(100, 100);
    expect(aim.note(130, 102)).toBe(true);
    expect(aim.note(150, 150)).toBe(true);
  });

  test("moving left is not", () => {
    const aim = createPointerAim();
    aim.note(100, 100);
    expect(aim.note(70, 100)).toBe(false);
  });

  test("hand jitter does not change the verdict", () => {
    const aim = createPointerAim();
    aim.note(100, 100);
    aim.note(140, 100);
    expect(aim.note(141, 103)).toBe(true);
    expect(aim.note(142, 140)).toBe(false);
  });

  test("reset forgets the direction", () => {
    const aim = createPointerAim();
    aim.note(100, 100);
    aim.note(140, 100);
    aim.reset();
    expect(aim.note(140, 100)).toBe(false);
  });
});
