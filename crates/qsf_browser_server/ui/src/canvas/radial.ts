export interface NeighborLayout {
  id: string;
  x: number;
  y: number;
  angle: number;
}

/**
 * Lay out neighbors evenly around a unit circle. Pure function: deterministic
 * for a given (neighbor_count, radius). Caller is responsible for centering.
 */
export function radialPositions(
  count: number,
  radius: number,
): NeighborLayout[] {
  if (count <= 0) return [];
  const out: NeighborLayout[] = [];
  const step = (Math.PI * 2) / count;
  for (let i = 0; i < count; i++) {
    const angle = -Math.PI / 2 + step * i;
    out.push({
      id: String(i),
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      angle,
    });
  }
  return out;
}
