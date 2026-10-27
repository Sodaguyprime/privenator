import type { ReactNode } from "react";
import { toRoman } from "../lib/roman";

/** Fixed paper grain + vignette. Render once, at the app root. */
export function Atmosphere() {
  return (
    <>
      <div className="vignette" aria-hidden="true" />
      <div className="paper-grain" aria-hidden="true" />
    </>
  );
}

export function Divider({ glyph = "✶", className = "" }: { glyph?: "✶" | "❧" | "✤" | "❦"; className?: string }) {
  return <div className={`ornate-divider ${className}`} data-glyph={glyph} aria-hidden="true" />;
}

/** "Volume III" overline flanked by short brass rules, or custom label text. */
export function VolumeLabel({ n = 1, children }: { n?: number; children?: ReactNode }) {
  return (
    <p className="inscription flex items-center justify-center gap-4">
      <span className="h-px w-10 bg-accent/60" aria-hidden="true" />
      {children ?? `Volume ${toRoman(n)}`}
      <span className="h-px w-10 bg-accent/60" aria-hidden="true" />
    </p>
  );
}
