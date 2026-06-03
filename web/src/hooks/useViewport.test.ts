import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import { useViewport } from "./useViewport";

describe("useViewport", () => {
  it("returns one of the three buckets", () => {
    const { result } = renderHook(() => useViewport());
    expect(["mobile", "tablet", "desktop"]).toContain(result.current);
  });
});
