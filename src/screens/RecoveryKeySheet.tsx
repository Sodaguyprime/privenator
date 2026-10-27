import { Copy, Download, ScrollText } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import { asCommandError, saveRecoveryKey } from "../lib/ipc";
import { toRoman } from "../lib/roman";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Frame } from "../ui/Frame";

/** How many groups the user must type back before continuing. */
const CHECK_GROUPS = 2;

/**
 * Shows the recovery key once, then asks for a couple of its groups back
 * (docs/SPEC.md "First run" step 3, and after "Recovery").
 */
export function RecoveryKeySheet({ recoveryKey, replacing, onConfirmed }: { recoveryKey: string; replacing: boolean; onConfirmed: () => void }) {
  const groups = recoveryKey.split("-");
  const [phase, setPhase] = useState<"show" | "confirm">("show");
  const [note, setNote] = useState<string | null>(null);

  // Pick the groups to check once; only full 4-character groups.
  const asked = useMemo(() => {
    const full = groups.map((g, i) => (g.length === 4 ? i : -1)).filter((i) => i >= 0);
    const picks: number[] = [];
    while (picks.length < CHECK_GROUPS) {
      const i = full[crypto.getRandomValues(new Uint32Array(1))[0] % full.length];
      if (!picks.includes(i)) picks.push(i);
    }
    return picks.sort((a, b) => a - b);
  }, [recoveryKey]); // eslint-disable-line react-hooks/exhaustive-deps

  async function copy() {
    try {
      await navigator.clipboard.writeText(recoveryKey);
      setNote("Copied. Paste it somewhere safe, then clear your clipboard.");
    } catch {
      setNote("Couldn't copy. Please write the key down instead.");
    }
  }

  async function save() {
    try {
      if (await saveRecoveryKey(recoveryKey)) setNote("Saved. Keep that file somewhere offline, such as a USB drive.");
    } catch (e) {
      setNote(asCommandError(e).message);
    }
  }

  if (phase === "confirm") {
    return <ConfirmKey groups={groups} asked={asked} onBack={() => setPhase("show")} onConfirmed={onConfirmed} />;
  }

  return (
    <Frame
      overline={replacing ? "A new key" : "Volume II"}
      title="Your recovery key"
      titleId="recovery-key-title"
      width="wide"
      medallion={<ScrollText className="h-6 w-6" strokeWidth={1.5} aria-hidden="true" />}
    >
      <p className="mx-auto max-w-xl text-lg text-muted-foreground">
        {replacing
          ? "Your old recovery key no longer works. This one replaces it."
          : "If you ever forget your password, this key is the only way back in."}{" "}
        <em className="text-foreground">It is shown once.</em> Write it down or save it, and keep it away from this computer.
      </p>

      {/* Groups are numbered so "type group V" in the check step is easy to find. */}
      <ol
        className="mx-auto my-8 flex max-w-2xl flex-wrap justify-center gap-y-4 rounded-default border border-accent/40 bg-background px-4 py-6 select-text"
        aria-label="Recovery key"
      >
        {groups.map((g, i) => (
          <li key={i} className="flex w-1/4 flex-col items-center gap-1">
            <span className="font-display text-xl tracking-[0.18em] text-accent md:text-2xl">{g}</span>
            <span className="font-display text-[0.625rem] tracking-[0.2em] text-muted-foreground" aria-hidden="true">
              {toRoman(i + 1)}
            </span>
          </li>
        ))}
      </ol>

      <div className="flex flex-col items-center justify-center gap-3 sm:flex-row">
        <Button variant="secondary" size="sm" onClick={copy}>
          <Copy className="h-4 w-4" strokeWidth={1.5} aria-hidden="true" /> Copy
        </Button>
        <Button variant="secondary" size="sm" onClick={save}>
          <Download className="h-4 w-4" strokeWidth={1.5} aria-hidden="true" /> Save as text file
        </Button>
      </div>
      <p className="mt-4 min-h-6 text-base text-muted-foreground" aria-live="polite">
        {note}
      </p>

      <div className="mt-6">
        <Button onClick={() => setPhase("confirm")}>I've kept it safe</Button>
      </div>
    </Frame>
  );
}

function ConfirmKey({ groups, asked, onBack, onConfirmed }: { groups: string[]; asked: number[]; onBack: () => void; onConfirmed: () => void }) {
  const [values, setValues] = useState<string[]>(asked.map(() => ""));
  const [error, setError] = useState<string | null>(null);

  // Same forgiveness as the Rust parser: case, O/0, I/L/1.
  const norm = (s: string) => s.trim().toUpperCase().replace(/O/g, "0").replace(/[IL]/g, "1");

  function submit(e: FormEvent) {
    e.preventDefault();
    if (asked.every((g, i) => norm(values[i]) === groups[g])) onConfirmed();
    else setError("That doesn't match. Check the key you kept and try again.");
  }

  return (
    <Frame overline="A brief examination" title="Check your copy" titleId="confirm-key-title">
      <p className="mx-auto mb-8 max-w-md text-lg text-muted-foreground">
        Type these groups from the key you kept. Groups are counted from the left.
      </p>
      <form onSubmit={submit} className="space-y-6">
        <div className="grid grid-cols-1 gap-6 sm:grid-cols-2">
          {asked.map((g, i) => (
            <Field
              key={g}
              label={`Group ${toRoman(g + 1)}`}
              value={values[i]}
              maxLength={4}
              autoFocus={i === 0}
              className="[&_input]:text-center [&_input]:font-display [&_input]:tracking-[0.3em] [&_input]:uppercase"
              onChange={(e) => setValues((v) => v.map((x, j) => (j === i ? e.target.value : x)))}
            />
          ))}
        </div>
        {error && (
          <p role="alert" className="text-base text-error">
            {error}
          </p>
        )}
        <div className="flex flex-col-reverse items-center justify-between gap-4 pt-2 sm:flex-row">
          <Button variant="ghost" onClick={onBack}>
            Show the key again
          </Button>
          <Button type="submit" disabled={values.some((v) => v.trim().length < 4)}>
            Continue
          </Button>
        </div>
      </form>
    </Frame>
  );
}
