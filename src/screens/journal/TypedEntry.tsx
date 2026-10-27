import { useState } from "react";
import * as ipc from "../../lib/ipc";
import { Button } from "../../ui/Button";
import { Divider, VolumeLabel } from "../../ui/Ornaments";

/** An entry written instead of spoken (docs/SPEC.md "Typed entries"). */
export function TypedEntry({ onSaved }: { onSaved: (id: string) => void }) {
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);

  async function save() {
    setSaving(true);
    onSaved(await ipc.createTypedEntry(text));
  }

  return (
    <section aria-labelledby="typed-title" className="mx-auto max-w-[72ch] px-6 py-10">
      <VolumeLabel>A written entry</VolumeLabel>
      <h1 id="typed-title" className="mt-5 text-center text-4xl tracking-tight">
        Set it down in ink
      </h1>
      <Divider glyph="❧" className="mx-auto my-8 w-56" />
      <textarea
        aria-label="Entry text"
        value={text}
        autoFocus
        spellCheck={false}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        placeholder="Write freely…"
        onChange={(e) => setText(e.target.value)}
        className="min-h-96 w-full resize-y rounded-default border border-border bg-background-alt p-5 font-body text-lg leading-relaxed text-foreground placeholder:italic placeholder:text-muted-foreground focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30"
      />
      <div className="mt-6 flex justify-center">
        <Button onClick={save} disabled={saving || !text.trim()}>
          {saving ? "Sealing…" : "Save entry"}
        </Button>
      </div>
    </section>
  );
}
