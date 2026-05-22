import type { AssociationDisplayEdge } from "../types";

export function computeNeighborIds(
  centerId: string,
  edges: AssociationDisplayEdge[],
): string[] {
  return Array.from(
    new Set(
      edges
        .flatMap((edge) => [edge.from_id, edge.to_id])
        .filter((id) => id !== centerId),
    ),
  );
}

export function maxEdgeWeight(edges: AssociationDisplayEdge[]): number {
  return Math.max(0.001, ...edges.map((edge) => edge.weight));
}

export function edgeWidth(weight: number, maxWeight: number): number {
  return 1 + (weight / maxWeight) * 3;
}
