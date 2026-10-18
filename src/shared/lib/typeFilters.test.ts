import { describe, expect, it } from "vitest";
import { TYPE_FILTERS, cycleTypeFilter } from "./typeFilters";

describe("cycleTypeFilter", () => {
  it("steps forward and back through the tabs", () => {
    expect(cycleTypeFilter(null, 1)).toBe("text");
    expect(cycleTypeFilter("text", 1)).toBe("image");
    expect(cycleTypeFilter("image", -1)).toBe("text");
  });

  it("wraps around at both ends", () => {
    expect(cycleTypeFilter(TYPE_FILTERS[TYPE_FILTERS.length - 1], 1)).toBeNull();
    expect(cycleTypeFilter(null, -1)).toBe(TYPE_FILTERS[TYPE_FILTERS.length - 1]);
  });

  it("treats a filter that has no tab as 全部", () => {
    expect(cycleTypeFilter("unknown", 1)).toBe("text");
  });
});
