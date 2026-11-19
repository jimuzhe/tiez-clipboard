import { describe, expect, it } from "vitest";
import { isReadableOn, parseCssColor, readableTextColor } from "./readableColor";

const white = { r: 255, g: 255, b: 255, a: 1 };
const dark = { r: 30, g: 30, b: 30, a: 1 };

describe("parseCssColor", () => {
  it("reads hex and rgb forms", () => {
    expect(parseCssColor("#fff")).toEqual(white);
    expect(parseCssColor("rgb(250, 249, 245)")).toEqual({ r: 250, g: 249, b: 245, a: 1 });
    expect(parseCssColor("rgb(0 0 0 / 50%)")).toEqual({ r: 0, g: 0, b: 0, a: 0.5 });
  });

  it("returns null for forms it does not handle", () => {
    expect(parseCssColor("white")).toBeNull();
    expect(parseCssColor("")).toBeNull();
  });
});

describe("readability", () => {
  it("flags near-white text copied from a dark app as unreadable on a white card", () => {
    expect(isReadableOn({ r: 250, g: 249, b: 245, a: 1 }, white)).toBe(false);
    expect(isReadableOn({ r: 20, g: 20, b: 20, a: 1 }, white)).toBe(true);
  });

  it("keeps the theme's text colour when it reads, else picks black or white", () => {
    const themed = { r: 17, g: 24, b: 39, a: 1 };
    expect(readableTextColor(white, themed)).toEqual(themed);
    expect(readableTextColor(dark, themed)).toEqual(white);
    expect(readableTextColor(white, null)).toEqual({ r: 0, g: 0, b: 0, a: 1 });
  });
});
