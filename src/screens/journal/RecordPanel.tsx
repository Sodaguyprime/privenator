import { Mic, Pause, Play, Square } from "lucide-react";
import { useEffect, useState } from "react";
import { formatDuration } from "../../lib/format";
import { useTauriEvent } from "../../lib/hooks";
import * as ipc from "../../lib/ipc";
import { Button } from "../../ui/Button";
import { LevelMeter } from "../../ui/LevelMeter";
import { MoodPicker } from "../../ui/MoodPicker";
import { Divider, VolumeLabel } from "../../ui/Ornaments";
import { TagInput } from "../../ui/TagInput";

type Phase =
  | { name: "idle"; error?: string }
  | { name: "starting" }
  | { name: "live"; paused: boolean }
  | { name: "stopping" }
  | { name: "save"; stopped: ipc.Stopped };

const MIC_ERRORS: Partial<Record<ipc.CommandError["kind"], string>> = {
  no_microphone: "No microphone was found. Connect one, then try again.",
  mic_blocked:
    "Microphone access is blocked. On Windows, open Settings › Privacy & security › Microphone and allow desktop apps to use it. On macOS, allow Privenator under System Settings › Privacy & Security › Microphone.",
  mic_error: "The microphone couldn't be opened. Check it isn't in use by another app, then try again.",
};

/**
 * Record → live transcript → stop → save dialog (docs/SPEC.md "Recording and
 * transcription"). Rebuilds itself from `recording_status` after an unlock,
 * since a recording keeps going while the journal is locked.
 */
export function RecordPanel({ onSaved, onBusyChange }: { onSaved: (id: string) => void; onBusyChange: (busy: boolean) => void }) {
  const [phase, setPhase] = useState<Phase>({ name: "starting" });
  const [finals, setFinals] = useState<ipc.DraftSegment[]>([]);
  const [preview, setPreview] = useState("");
  const [level, setLevel] = useState<ipc.Level>({ level: 0, elapsed_ms: 0, paused: false });

  useEffect(() => {
    ipc.recordingStatus().then((s) => {
      setFinals(s.finals);
      setLevel((l) => ({ ...l, elapsed_ms: s.elapsed_ms }));
      if (s.state === "recording" || s.state === "paused") setPhase({ name: "live", paused: s.state === "paused" });
      else if (s.state === "stopped" && s.stopped) setPhase({ name: "save", stopped: s.stopped });
      else setPhase({ name: "idle" });
    });
  }, []);

  const busy = phase.name !== "idle";
  useEffect(() => onBusyChange(busy), [busy, onBusyChange]);

  useTauriEvent(ipc.onLevel, setLevel);
  useTauriEvent(ipc.onPreview, setPreview);
  useTauriEvent(ipc.onFinal, (segment) => {
    setFinals((f) => [...f, segment]);
    setPreview("");
  });

  async function start() {
    setPhase({ name: "starting" });
    setFinals([]);
    setPreview("");
    try {
      await ipc.startRecording();
      setPhase({ name: "live", paused: false });
    } catch (e) {
      const err = ipc.asCommandError(e);
      setPhase({ name: "idle", error: MIC_ERRORS[err.kind] ?? err.message });
    }
  }

  async function togglePause(paused: boolean) {
    await (paused ? ipc.resumeRecording() : ipc.pauseRecording());
    setPhase({ name: "live", paused: !paused });
  }

  async function stop() {
    setPhase({ name: "stopping" });
    try {
      const stopped = await ipc.stopRecording();
      setPreview("");
      setPhase({ name: "save", stopped });
    } catch (e) {
      setPhase({ name: "idle", error: ipc.asCommandError(e).message });
    }
  }

  if (phase.name === "save") return <SaveDialog stopped={phase.stopped} onSaved={onSaved} />;

  const live = phase.name === "live";
  return (
    <section aria-labelledby="record-title" className="mx-auto flex max-w-3xl flex-col items-center px-6 py-12 text-center">
      <VolumeLabel>{live ? (phase.paused ? "Paused" : "Recording") : "A new entry"}</VolumeLabel>
      <h1 id="record-title" className="mt-5 text-4xl tracking-tight md:text-5xl">
        {live || phase.name === "stopping" ? <span className="engraved font-display tabular-nums">{formatDuration(level.elapsed_ms / 1000)}</span> : "Speak freely"}
      </h1>
      <Divider glyph="❧" className="mx-auto my-8 w-56" />

      {phase.name === "idle" || phase.name === "starting" ? (
        <>
          <button
            type="button"
            onClick={start}
            disabled={phase.name === "starting"}
            aria-label="Begin recording"
            className="wax-seal h-28 w-28 cursor-pointer transition-[filter] duration-(--duration-base) hover:brightness-110 disabled:opacity-60"
          >
            <Mic className="h-10 w-10" strokeWidth={1.5} aria-hidden="true" />
          </button>
          <p className="mt-6 max-w-md text-lg text-muted-foreground">
            {phase.name === "starting" ? "Opening the microphone…" : "Your words are transcribed on this machine as you talk, and sealed as they're written."}
          </p>
          {phase.name === "idle" && phase.error && (
            <p role="alert" className="mt-4 max-w-lg text-base text-error">
              {phase.error}
            </p>
          )}
        </>
      ) : (
        <>
          <div className="flex items-center gap-6">
            <span className={`wax-seal h-12 w-12 ${live && !phase.paused ? "motion-safe:animate-pulse" : ""}`} aria-hidden="true">
              <Mic className="h-5 w-5" strokeWidth={1.5} />
            </span>
            <LevelMeter level={live && !phase.paused ? level.level : 0} />
          </div>
          <div className="mt-8 flex gap-4">
            {live && (
              <Button variant="secondary" onClick={() => togglePause(phase.paused)}>
                {phase.paused ? <Play className="h-4 w-4" aria-hidden="true" /> : <Pause className="h-4 w-4" aria-hidden="true" />}
                {phase.paused ? "Resume" : "Pause"}
              </Button>
            )}
            <Button onClick={stop} disabled={phase.name === "stopping"}>
              <Square className="h-4 w-4" aria-hidden="true" />
              {phase.name === "stopping" ? "Finishing transcription…" : "Stop"}
            </Button>
          </div>
          <Transcript finals={finals} preview={preview} />
        </>
      )}
    </section>
  );
}

function Transcript({ finals, preview }: { finals: ipc.DraftSegment[]; preview: string }) {
  if (!finals.length && !preview) {
    return <p className="mt-10 text-lg italic text-muted-foreground">Text appears here a moment after you speak.</p>;
  }
  return (
    <div aria-live="polite" className="mt-10 w-full rounded-default border border-border bg-background-alt/50 p-6 text-left text-lg leading-relaxed">
      {finals.map((s) => (
        <span key={s.t_ms}>{s.text} </span>
      ))}
      {preview && <span className="italic text-muted-foreground">{preview}</span>}
    </div>
  );
}

function SaveDialog({ stopped, onSaved }: { stopped: ipc.Stopped; onSaved: (id: string) => void }) {
  const [keepAudio, setKeepAudio] = useState<boolean | null>(null);
  const [mood, setMood] = useState<number | null>(null);
  const [tags, setTags] = useState<string[]>([]);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    ipc.getSettings().then((s) => setKeepAudio((k) => k ?? s.default_keep_audio));
    ipc.allTags().then(setSuggestions);
  }, []);

  async function save() {
    setSaving(true);
    try {
      onSaved(await ipc.saveRecording(keepAudio ?? false, mood, tags));
    } catch (e) {
      setError(ipc.asCommandError(e).message);
      setSaving(false);
    }
  }

  return (
    <section aria-labelledby="save-title" className="mx-auto max-w-2xl px-6 py-12">
      <div className="text-center">
        <VolumeLabel>{formatDuration(stopped.duration_s)} recorded</VolumeLabel>
        <h1 id="save-title" className="mt-5 text-4xl tracking-tight">
          Seal this entry
        </h1>
        <Divider glyph="❧" className="mx-auto my-8 w-56" />
      </div>

      <div className="max-h-60 overflow-y-auto rounded-default border border-border bg-background-alt/50 p-5 text-lg leading-relaxed">
        {stopped.text || <span className="italic text-muted-foreground">No speech was recognised.</span>}
      </div>

      <div className="mt-8 space-y-6">
        <label className="flex cursor-pointer items-start gap-3 text-left text-lg">
          <input type="checkbox" className="mt-1.5 h-4 w-4 accent-(--color-accent)" checked={keepAudio ?? false} onChange={(e) => setKeepAudio(e.target.checked)} />
          <span>
            Keep the audio
            <span className="block text-base text-muted-foreground">Encrypted with the entry, so you can listen back. Otherwise only the text is kept.</span>
          </span>
        </label>
        <MoodPicker value={mood} onChange={setMood} />
        <TagInput value={tags} onChange={setTags} suggestions={suggestions} />
      </div>

      {error && (
        <p role="alert" className="mt-6 text-base text-error">
          {error}
        </p>
      )}
      <div className="mt-10 flex justify-center">
        <Button size="lg" onClick={save} disabled={saving || keepAudio === null}>
          {saving ? "Sealing…" : "Save entry"}
        </Button>
      </div>
    </section>
  );
}
