import { BookLock } from "lucide-react";
import { Button } from "../ui/Button";
import { Divider, VolumeLabel } from "../ui/Ornaments";

/** First-run welcome (docs/SPEC.md, "First run" step 1). */
export function Welcome({ version, onBegin, onRestore }: { version?: string; onBegin: () => void; onRestore: () => void }) {
  return (
    <main className="grid min-h-full place-items-center px-4 py-10">
      <div className="w-full max-w-3xl">
        <section
          aria-labelledby="welcome-title"
          className="flourish flourish-lg rounded-default border border-border bg-background-alt/50 px-6 py-10 text-center [--bg-behind:#211b17] md:px-12"
        >
          <div className="mx-auto mb-6 grid h-14 w-14 place-items-center rounded-full border border-accent/30 bg-background">
            <BookLock className="h-7 w-7 text-accent" strokeWidth={1.5} aria-hidden="true" />
          </div>

          <VolumeLabel>Ex libris · Privenator</VolumeLabel>

          <h1 id="welcome-title" className="mt-6 text-5xl tracking-tight md:text-6xl">
            A journal that keeps <em className="text-accent">its own counsel</em>
          </h1>

          <Divider glyph="❧" className="mx-auto my-8 w-64" />

          <div className="mx-auto max-w-xl space-y-5 text-left text-lg text-muted-foreground">
            <p className="drop-cap">
              Speak, and your words are set down here, transcribed on this machine and sealed with a password only you
              hold. Nothing leaves this device: not to a server, and not to us.
            </p>
            <p>
              That privacy cuts both ways.{" "}
              <em className="text-foreground">
                If you lose both your password and your recovery key, no one can open this journal again.
              </em>
            </p>
          </div>

          <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
            <Button onClick={onBegin}>
              Begin a new journal
            </Button>
            <Button variant="secondary" onClick={onRestore}>
              Restore from backup
            </Button>
          </div>
        </section>

        {version && <p className="inscription mt-8 text-center text-muted-foreground">Edition {version}</p>}
      </div>
    </main>
  );
}
