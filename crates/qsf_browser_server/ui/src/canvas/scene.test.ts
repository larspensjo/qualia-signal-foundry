import { describe, expect, it } from "vitest";
import type { AssociationDisplayEdge } from "../types";
import { computeNeighborIds, edgeWidth, maxEdgeWeight } from "./scene";

function edge(
  from_id: string,
  to_id: string,
  weight: number,
): AssociationDisplayEdge {
  return {
    from_id,
    to_id,
    weight,
    last_reinforced_at: "2026-05-22T00:00:00Z",
    reason: "test",
  };
}

describe("computeNeighborIds", () => {
  it("deduplicates neighbors and filters the center id", () => {
    expect(
      computeNeighborIds("a", [
        edge("a", "b", 1),
        edge("b", "a", 0.5),
        edge("a", "ghost", 0.25),
        edge("a", "a", 0.1),
      ]),
    ).toEqual(["b", "ghost"]);
  });
});

describe("edge width helpers", () => {
  it("scales edge widths linearly from the maximum weight", () => {
    const edges = [edge("a", "b", 0.5), edge("a", "c", 1)];
    const max = maxEdgeWeight(edges);

    expect(max).toBe(1);
    expect(edgeWidth(0.5, max)).toBeCloseTo(2.5);
    expect(edgeWidth(1, max)).toBeCloseTo(4);
  });

  it("keeps zero-weight edges at the base width", () => {
    const max = maxEdgeWeight([edge("a", "b", 0)]);

    expect(max).toBe(0.001);
    expect(edgeWidth(0, max)).toBe(1);
  });
});
