import { FloodPreviewEngine, validationBadgeLabel } from "./preview";

export { validationBadgeLabel };

/**
 * Compact badge for GeoViewport / status — “Live preview” until ANUGA validates.
 */
export function PreviewBadge({
  validation = "live",
  backend,
}: {
  validation?: "live" | "comparing" | "validated" | "diverged";
  backend?: "webgpu" | "webgl" | "cpu" | null;
}) {
  const label = validationBadgeLabel(validation);
  const tone =
    validation === "validated"
      ? "text-[var(--color-hydro)] border-[var(--color-hydro)]/40"
      : validation === "diverged"
        ? "text-[var(--color-critical)] border-[var(--color-critical)]/40"
        : "text-[var(--color-gauge)] border-[var(--color-gauge)]/35";

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded border bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide backdrop-blur-sm ${tone}`}
      title={backend ? `Backend: ${backend}` : undefined}
      role="status"
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          validation === "validated"
            ? "bg-[var(--color-hydro)]"
            : validation === "diverged"
              ? "bg-[var(--color-critical)]"
              : "bg-[var(--color-gauge)]"
        }`}
        aria-hidden
      />
      {label}
      {backend && (
        <span className="font-mono text-[9px] font-normal normal-case tracking-normal text-ink-dim">
          {backend}
        </span>
      )}
    </span>
  );
}

/** Re-export for callers that need the engine class beside the badge. */
export { FloodPreviewEngine };
