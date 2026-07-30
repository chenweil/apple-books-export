export type LayoutMode = "compact" | "medium" | "wide";

export function getLayoutMode(width: number): LayoutMode {
  if (width < 60) return "compact";
  if (width < 100) return "medium";
  return "wide";
}
