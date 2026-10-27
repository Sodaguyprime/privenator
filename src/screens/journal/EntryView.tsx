import { Pencil, Trash2, Volume2 } from "lucide-react";
import { useEffect, useState } from "react";
import { formatDay, formatDuration, formatTime } from "../../lib/format";
import * as ipc from "../../lib/ipc";
import { Button } from "../../ui/Button";
import { MoodPicker } from "../../ui/MoodPicker";
import { Divider } from "../../ui/Ornaments";
import { TagInput } from "../../ui/TagInput";

/**
 * One entry: read, edit (original kept), mood, tags, audio, delete
 * (docs/SPEC.md "Entries"). Plaintext lives only in this component's state
 * and is dropped when it unmounts (e.g. on lock).
 */
export function EntryView({ id, onChanged, onDeleted }: { id: string; onChanged: () => void; onDeleted: () => void }) {
  const [entry, setEntry] = useState<ipc.Entry | null>(null);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let current = true;
    ipc.getEntry(id).then(
      (e) => current && setEntry(e),
      (e) => current && setError(ipc.asCommandError(e).message),
    );
    return () => {
      current = false;
    };
  }, [id]);

  if (error) return <p className="p-10 text-error">{error}</p>;
  if (!entry) return null;
  // Remount per entry so editing state never leaks between entries.
  return <EntryBody key={entry.id} entry={entry} onChanged={onChanged} onDeleted={onDeleted} />;
}

function EntryBody({ entry: initial, onChanged, onDeleted }: { entry: ipc.Entry; onChanged: () => void; onDeleted: () => void }) {
  const [entry, setEntry] = useState(initial);
  const [title, setTitle] = useState(initial.title ?? "");
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [showOriginal, setShowOriginal] = useState(false);
  const [confirm, setConfirm] = useState<"delete" | "revert" | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const current = entry.edited_text ?? entry.original_text;

  useEffect(() => {
    ipc.allTags().then(setSuggestions);
  }, []);

  async function refresh() {
    setEntry(await ipc.getEntry(entry.id));
    onChanged();
  }

  async function saveTitle() {
    if (title.trim() === (entry.title ?? "")) return;
    await ipc.setEntryTitle(entry.id, title.trim() || null);
    await refresh();
  }

  async function saveText() {
    await ipc.setEntryText(entry.id, draft);
    setEditing(false);
    await refresh();
  }

  return (
    <article aria-labelledby="entry-title" className="mx-auto max-w-[72ch] px-6 py-10">
      <p className="inscription text-muted-foreground">
        {formatDay(entry.created_at)} · {formatTime(entry.created_at)}
        {entry.duration_s > 0 && <> · {formatDuration(entry.duration_s)}</>}
      </p>
      <h1 id="entry-title" className="mt-3">
        <input
          aria-label="Title"
          value={title}
          placeholder="Untitled"
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => setTitle(e.target.value)}
          onBlur={saveTitle}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className="w-full bg-transparent font-heading text-4xl tracking-tight text-foreground placeholder:italic placeholder:text-muted-foreground focus:outline-none md:text-5xl"
        />
      </h1>
      <Divider glyph="✶" className="my-6 w-40" />

      {entry.has_audio && <AudioPlayer id={entry.id} />}

      {editing ? (
        <div>
          <textarea
            aria-label="Entry text"
            value={draft}
            autoFocus
            spellCheck={false}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if ((e.ctrlKey || e.metaKey) && e.key === "s") {
                e.preventDefault();
                saveText();
              }
            }}
            className="min-h-80 w-full resize-y rounded-default border border-border bg-background-alt p-5 font-body text-lg leading-relaxed text-foreground focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30"
          />
          <div className="mt-4 flex gap-3">
            <Button size="sm" onClick={saveText}>
              Save edit
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
              Cancel
            </Button>
          </div>
        </div>
      ) : (
        <>
          <Reading text={showOriginal ? entry.original_text : current} />
          <div className="mt-6 flex flex-wrap items-center gap-x-4 gap-y-2">
            {!showOriginal && (
              <Button size="sm" variant="secondary" onClick={() => (setDraft(current), setEditing(true))}>
                <Pencil className="h-4 w-4" aria-hidden="true" /> Edit
              </Button>
            )}
            {entry.edited_text !== null && (
              <>
                <Button variant="ghost" onClick={() => setShowOriginal((v) => !v)}>
                  {showOriginal ? "View edited" : "View original"}
                </Button>
                <Button variant="ghost" onClick={() => setConfirm("revert")}>
                  Revert to original
                </Button>
              </>
            )}
          </div>
          {showOriginal && <p className="mt-2 text-base italic text-muted-foreground">The original transcript, as first saved. It never changes.</p>}
        </>
      )}

      <div className="mt-10 space-y-6 border-t border-border pt-8">
        <MoodPicker value={entry.mood} onChange={async (mood) => (await ipc.setEntryMood(entry.id, mood), await refresh())} />
        <TagInput value={entry.tags} suggestions={suggestions} onChange={async (tags) => (await ipc.setEntryTags(entry.id, tags), await refresh())} />
      </div>

      <div className="mt-10 border-t border-border pt-6">
        {confirm ? (
          <div role="alertdialog" aria-label="Confirm" className="flex flex-wrap items-center gap-4">
            <p className="text-lg">{confirm === "delete" ? "Delete this entry, its tags and its audio? This can't be undone." : "Discard your edits and go back to the original transcript?"}</p>
            <Button
              size="sm"
              variant="secondary"
              onClick={async () => {
                if (confirm === "delete") {
                  await ipc.deleteEntry(entry.id);
                  onDeleted();
                } else {
                  await ipc.revertEntry(entry.id);
                  setConfirm(null);
                  setShowOriginal(false);
                  await refresh();
                }
              }}
            >
              {confirm === "delete" ? "Delete" : "Revert"}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirm(null)}>
              Keep
            </Button>
          </div>
        ) : (
          <Button variant="ghost" onClick={() => setConfirm("delete")}>
            <Trash2 className="h-4 w-4" aria-hidden="true" /> Delete entry
          </Button>
        )}
      </div>
    </article>
  );
}

/** Reading view: paragraphs, with a drop cap on the first. */
function Reading({ text }: { text: string }) {
  const paragraphs = text.split(/\n{2,}|\r\n\r\n/).filter((p) => p.trim());
  if (!paragraphs.length) return <p className="text-lg italic text-muted-foreground">Nothing was said in this entry.</p>;
  return (
    <div className="space-y-4 text-lg leading-relaxed">
      {paragraphs.map((p, i) => (
        <p key={i} className={`whitespace-pre-wrap ${i === 0 ? "drop-cap" : ""}`}>
          {p}
        </p>
      ))}
    </div>
  );
}

/** Decrypts on demand into an in-memory blob: URL, revoked on unmount. */
function AudioPlayer({ id }: { id: string }) {
  const [url, setUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => () => void (url && URL.revokeObjectURL(url)), [url]);

  async function load() {
    setLoading(true);
    try {
      const bytes = await ipc.entryAudio(id);
      setUrl(URL.createObjectURL(new Blob([bytes], { type: "audio/ogg; codecs=opus" })));
    } catch (e) {
      setError(ipc.asCommandError(e).message);
    } finally {
      setLoading(false);
    }
  }

  if (url) return <audio controls autoPlay src={url} className="mb-8 w-full" />;
  return (
    <div className="mb-8">
      <Button size="sm" variant="secondary" onClick={load} disabled={loading}>
        <Volume2 className="h-4 w-4" aria-hidden="true" /> {loading ? "Opening…" : "Play recording"}
      </Button>
      {error && <p className="mt-2 text-base text-error">{error}</p>}
    </div>
  );
}
