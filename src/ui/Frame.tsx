import type { ReactNode } from "react";
import { Divider, VolumeLabel } from "./Ornaments";

type Props = {
  /** Small Cinzel line above the title, e.g. "Volume I" or "Ex libris". */
  overline: ReactNode;
  title: ReactNode;
  titleId: string;
  /** Centred medallion above the overline (an icon). */
  medallion?: ReactNode;
  width?: "narrow" | "wide";
  children: ReactNode;
  footer?: ReactNode;
};

/**
 * A framed, centred "certificate" screen with 40 px brass corners.
 * Used for every screen outside the journal itself (docs/DESIGN.md).
 */
export function Frame({ overline, title, titleId, medallion, width = "narrow", children, footer }: Props) {
  return (
    <main className="grid min-h-full place-items-center px-4 py-10">
      <div className={`w-full ${width === "wide" ? "max-w-3xl" : "max-w-xl"}`}>
        <section
          aria-labelledby={titleId}
          className="flourish flourish-lg rounded-default border border-border bg-background-alt/50 px-6 py-10 text-center [--bg-behind:#211b17] md:px-12"
        >
          {medallion && (
            <div className="mx-auto mb-6 grid h-14 w-14 place-items-center rounded-full border border-accent/30 bg-background text-accent">
              {medallion}
            </div>
          )}
          <VolumeLabel>{overline}</VolumeLabel>
          <h1 id={titleId} className="mt-5 text-4xl tracking-tight md:text-5xl">
            {title}
          </h1>
          <Divider glyph="❧" className="mx-auto my-8 w-56" />
          {children}
        </section>
        {footer}
      </div>
    </main>
  );
}
