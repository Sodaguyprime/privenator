import { Lock } from "lucide-react";
import { useState, type FormEvent } from "react";
import { useSecondsUntil } from "../lib/hooks";
import { asCommandError, unlock } from "../lib/ipc";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Frame } from "../ui/Frame";

/** Single password field; wrong attempts add a delay (docs/SPEC.md "Unlock"). */
export function Unlock({ onUnlocked, onForgot }: { onUnlocked: (recovered: number) => void; onForgot: () => void }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [waitUntil, setWaitUntil] = useState(0);
  const wait = useSecondsUntil(waitUntil);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!password || busy || wait > 0) return;
    setBusy(true);
    setError(null);
    try {
      const { recovered } = await unlock(password);
      setPassword("");
      onUnlocked(recovered);
    } catch (err) {
      const ce = asCommandError(err);
      setError(
        ce.kind === "wrong_password" ? "That password didn't open the journal." : ce.kind === "throttled" ? null : ce.message,
      );
      if (ce.retry_after_ms) setWaitUntil(Date.now() + ce.retry_after_ms);
      setBusy(false);
    }
  }

  return (
    <Frame overline="Ex libris · Privenator" title="Your journal is locked" titleId="unlock-title" medallion={<Lock className="h-6 w-6" strokeWidth={1.5} aria-hidden="true" />}>
      <form onSubmit={submit} className="space-y-6 text-left">
        <Field
          label="Password"
          type="password"
          value={password}
          autoFocus
          onChange={(e) => setPassword(e.target.value)}
          error={error}
          hint={wait > 0 ? `You can try again in ${wait} s.` : null}
        />
        <div className="flex flex-col-reverse items-center justify-between gap-4 sm:flex-row">
          <Button variant="ghost" onClick={onForgot} disabled={busy}>
            Forgot password?
          </Button>
          <Button type="submit" disabled={!password || busy || wait > 0} className="w-full sm:w-auto">
            {busy ? "Opening…" : "Unlock"}
          </Button>
        </div>
      </form>
    </Frame>
  );
}
