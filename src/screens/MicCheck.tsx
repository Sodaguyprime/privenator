import { Mic } from "lucide-react";
import { useEffect, useState } from "react";
import { useTauriEvent } from "../lib/hooks";
import * as ipc from "../lib/ipc";
import { Button } from "../ui/Button";
import { Frame } from "../ui/Frame";
import { LevelMeter } from "../ui/LevelMeter";

const BLOCKED_HELP =
  "Microphone access is blocked. On Windows: Settings › Privacy & security › Microphone, and allow desktop apps. On macOS: System Settings › Privacy & Security › Microphone.";

/**
 * Microphone choice plus a live level meter. Nothing is recorded or stored;
 * the test stops when this unmounts. Saves the choice as `mic_device`.
 */
export function MicTester() {
  const [devices, setDevices] = useState<ipc.InputDevice[]>([]);
  const [device, setDevice] = useState<string | null>(null);
  const [level, setLevel] = useState(0);
  const [error, setError] = useState<string>();

  useEffect(() => {
    ipc.listMicrophones().then(setDevices);
    ipc.getSettings().then((s) => setDevice(s.mic_device));
  }, []);

  useEffect(() => {
    let cancelled = false;
    ipc.micTestStart(device).then(
      () => !cancelled && setError(undefined),
      (e) => {
        if (cancelled) return;
        const err = ipc.asCommandError(e);
        setError(err.kind === "mic_blocked" ? BLOCKED_HELP : err.kind === "no_microphone" ? "No microphone was found." : err.message);
      },
    );
    return () => {
      cancelled = true;
      ipc.micTestStop();
    };
  }, [device]);

  useTauriEvent(ipc.onLevel, (l) => setLevel(l.level));

  async function choose(id: string) {
    const next = id || null;
    setDevice(next);
    await ipc.updateSettings({ mic_device: next });
  }

  return (
    <div className="space-y-4 text-left">
      <label className="block">
        <span className="mb-2 block font-display text-[0.6875rem] font-medium uppercase tracking-[0.2em] text-muted-foreground">Microphone</span>
        <select value={device ?? ""} onChange={(e) => choose(e.target.value)} className="h-12 w-full rounded-default border border-border bg-background-alt px-3 font-body text-lg text-foreground focus:border-accent focus:outline-none">
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
              {d.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>
      </label>
      <div className="flex items-center gap-4">
        <LevelMeter level={error ? 0 : level} />
        <span className="text-base text-muted-foreground">{error ? "" : "Say something; the bars should move."}</span>
      </div>
      {error && (
        <p role="alert" className="text-base text-error">
          {error}
        </p>
      )}
    </div>
  );
}

/** First run, step 4 (docs/SPEC.md "First run"): check the microphone. */
export function MicCheck({ onDone }: { onDone: () => void }) {
  return (
    <Frame overline="Volume III" title="Test your microphone" titleId="mic-title" medallion={<Mic className="h-6 w-6" strokeWidth={1.5} aria-hidden="true" />}>
      <p className="mx-auto mb-8 max-w-md text-lg text-muted-foreground">Nothing is recorded here. You can change the microphone later in Settings.</p>
      <MicTester />
      <div className="mt-10">
        <Button onClick={onDone}>Open the journal</Button>
      </div>
    </Frame>
  );
}
