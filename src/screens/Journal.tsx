import { ArrowLeft, Feather, Lock, Mic, Settings } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import * as ipc from "../lib/ipc";
import { EntryView } from "./journal/EntryView";
import { RecordPanel } from "./journal/RecordPanel";
import { SettingsPanel } from "./journal/SettingsPanel";
import { Sidebar } from "./journal/Sidebar";
import { TypedEntry } from "./journal/TypedEntry";
import { VolumeLabel } from "../ui/Ornaments";

type View = { kind: "home" } | { kind: "entry"; id: string } | { kind: "record" } | { kind: "typed" } | { kind: "settings" };

/**
 * The open journal: entries sidebar plus the main pane (docs/DESIGN.md
 * "Layout"). Below 768 px it is one column: the list, or the open pane with a
 * back button. Unmounted on lock, which drops all plaintext it held.
 */
export function Journal({ recovered, onLock, onRestored }: { recovered: number; onLock: () => void; onRestored: () => void }) {
  const [view, setView] = useState<View>({ kind: "home" });
  const [listVersion, setListVersion] = useState(0);
  const [recordingBusy, setRecordingBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(
    recovered ? `${recovered} recording${recovered > 1 ? "s were" : " was"} cut short last time and ${recovered > 1 ? "have" : "has"} been saved as ${recovered > 1 ? "entries" : "an entry"}.` : null,
  );
  const refresh = useCallback(() => setListVersion((v) => v + 1), []);

  // A recording survives locking: reopen its panel after an unlock.
  useEffect(() => {
    ipc.recordingStatus().then((s) => s.state !== "idle" && setView({ kind: "record" }));
    ipc.getSettings().then((s) => {
      if (s.backup_reminder_days && Date.now() - (s.last_backup_at ?? 0) > s.backup_reminder_days * 86_400_000) {
        setNotice((n) => n ?? "It's time for a backup. You can make one in Settings.");
      }
    });
  }, []);

  const open = (v: View) => !recordingBusy && setView(v);
  const selectedId = view.kind === "entry" ? view.id : null;

  return (
    <div className="flex h-full">
      <aside className={`w-full shrink-0 flex-col border-r border-border bg-background-alt/30 md:flex md:w-80 ${view.kind === "home" ? "flex" : "hidden"}`}>
        <header className="flex items-center justify-between gap-2 border-b border-border px-4 py-3">
          <VolumeLabel>Privenator</VolumeLabel>
          <div className="flex gap-1">
            <IconButton label="Settings" onClick={() => open({ kind: "settings" })} disabled={recordingBusy}>
              <Settings className="h-4 w-4" />
            </IconButton>
            <IconButton label="Lock now" onClick={onLock}>
              <Lock className="h-4 w-4" />
            </IconButton>
          </div>
        </header>
        <div className="flex gap-2 border-b border-border p-4">
          <button
            type="button"
            onClick={() => setView({ kind: "record" })}
            className="wax-seal flex h-12 flex-1 cursor-pointer items-center justify-center gap-2 rounded-default font-display text-xs uppercase tracking-[0.15em] hover:brightness-110"
          >
            <Mic className="h-4 w-4" aria-hidden="true" /> {recordingBusy ? "Recording…" : "Record"}
          </button>
          <IconButton label="Write an entry" onClick={() => open({ kind: "typed" })} disabled={recordingBusy} large>
            <Feather className="h-5 w-5" />
          </IconButton>
        </div>
        <div className="min-h-0 flex-1">
          <Sidebar version={listVersion} selectedId={selectedId} onSelect={(id) => open({ kind: "entry", id })} />
        </div>
      </aside>

      <main className={`min-w-0 flex-1 overflow-y-auto ${view.kind === "home" ? "hidden md:block" : ""}`}>
        {view.kind !== "home" && !recordingBusy && (
          <button type="button" onClick={() => setView({ kind: "home" })} className="m-4 inline-flex items-center gap-2 font-display text-xs uppercase tracking-[0.15em] text-accent md:hidden">
            <ArrowLeft className="h-4 w-4" aria-hidden="true" /> Entries
          </button>
        )}
        {notice && (
          <div role="status" className="mx-6 mt-6 flex items-start justify-between gap-4 rounded-default border border-accent/40 bg-accent/10 px-4 py-3 text-base">
            <span>{notice}</span>
            <button type="button" onClick={() => setNotice(null)} className="font-display text-xs uppercase tracking-[0.15em] text-accent">
              Dismiss
            </button>
          </div>
        )}
        {view.kind === "home" && <Home />}
        {view.kind === "record" && (
          <RecordPanel
            onBusyChange={setRecordingBusy}
            onSaved={(id) => {
              setRecordingBusy(false);
              refresh();
              setView({ kind: "entry", id });
            }}
          />
        )}
        {view.kind === "typed" && (
          <TypedEntry
            onSaved={(id) => {
              refresh();
              setView({ kind: "entry", id });
            }}
          />
        )}
        {view.kind === "entry" && (
          <EntryView
            id={view.id}
            onChanged={refresh}
            onDeleted={() => {
              refresh();
              setView({ kind: "home" });
            }}
          />
        )}
        {view.kind === "settings" && <SettingsPanel onRestored={onRestored} />}
      </main>
    </div>
  );
}

function Home() {
  return (
    <div className="grid h-full place-items-center px-6 text-center">
      <div>
        <p className="font-heading text-3xl italic text-muted-foreground">Choose an entry, or begin a new one.</p>
      </div>
    </div>
  );
}

function IconButton({ label, onClick, disabled, large, children }: { label: string; onClick: () => void; disabled?: boolean; large?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className={`grid place-items-center rounded-default border border-border text-accent transition-colors duration-(--duration-base) hover:border-accent hover:text-accent-bright disabled:opacity-50 ${
        large ? "h-12 w-12" : "h-9 w-9"
      }`}
    >
      {children}
    </button>
  );
}
