import { describe, expect, it } from "vitest";
import { radialPositions } from "./radial";

describe("radialPositions", () => {
  it("places the first neighbor at the top and spreads the rest evenly", () => {
    const positions = radialPositions(4, 10);

    expect(positions).toHaveLength(4);
    expect(positions[0].id).toBe("0");
    expect(positions[0].x).toBeCloseTo(0);
    expect(positions[0].y).toBeCloseTo(-10);
    expect(positions[0].angle).toBeCloseTo(-Math.PI / 2);
    expect(positions[1].x).toBeCloseTo(10);
    expect(positions[1].y).toBeCloseTo(0);
    expect(positions[1].angle).toBeCloseTo(0);
    expect(positions[2].x).toBeCloseTo(0);
    expect(positions[2].y).toBeCloseTo(10);
    expect(positions[2].angle).toBeCloseTo(Math.PI / 2);
    expect(positions[3].x).toBeCloseTo(-10);
    expect(positions[3].y).toBeCloseTo(0);
    expect(positions[3].angle).toBeCloseTo(Math.PI);
  });

  it("places a single neighbor straight up", () => {
    const positions = radialPositions(1, 10);

    expect(positions).toHaveLength(1);
    expect(positions[0].x).toBeCloseTo(0);
    expect(positions[0].y).toBeCloseTo(-10);
    expect(positions[0].angle).toBeCloseTo(-Math.PI / 2);
  });

  it("supports the production neighborhood limit", () => {
    const positions = radialPositions(8, 10);

    expect(positions).toHaveLength(8);
    expect(positions[0].y).toBeCloseTo(-10);
    expect(positions[4].y).toBeCloseTo(10);
  });

  it("collapses positions to the origin when radius is zero", () => {
    for (const position of radialPositions(8, 0)) {
      expect(position.x).toBeCloseTo(0);
      expect(position.y).toBeCloseTo(0);
    }
  });

  it("returns no positions for empty neighborhoods", () => {
    expect(radialPositions(0, 10)).toEqual([]);
    expect(radialPositions(-1, 10)).toEqual([]);
  });
});
