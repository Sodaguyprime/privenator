import { KeyRound } from "lucide-react";
import { useState, type FormEvent } from "react";
import { asCommandError } from "../lib/ipc";
import { Button } from "../ui/Button";
import { usePasswordStrength } from "../lib/hooks";
import { Field, NewPasswordField } from "../ui/Field";
import { Frame } from "../ui/Frame";

type Props = {
  overline: string;
  title: string;
  intro: string;
  submitLabel: string;
  busyLabel: string;
  /** Does the work (create or recover) and resolves to the recovery key. */
  onSubmit: (password: string) => Promise<string>;
  onDone: (recoveryKey: string) => void;
  onBack?: () => void;
  /** Extra fields above the password (the recovery key, when recovering). */
  children?: React.ReactNode;
  canSubmit?: boolean;
};

/**
 * Choose a password, entered twice, with the strength meter
 * (docs/SPEC.md "First run" step 2, and "Recovery").
 */
export function CreatePassword({ overline, title, intro, submitLabel, busyLabel, onSubmit, onDone, onBack, children, canSubmit = true }: Props) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const strength = usePasswordStrength(password);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mismatch = confirm.length > 0 && confirm !== password;
  const ready = canSubmit && !!strength?.acceptable && confirm === password && !busy;

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    setError(null);
    try {
      const key = await onSubmit(password);
      setPassword("");
      setConfirm("");
      onDone(key);
    } catch (err) {
      setError(asCommandError(err).message);
      setBusy(false);
    }
  }

  return (
    <Frame overline={overline} title={title} titleId="create-password-title" medallion={<KeyRound className="h-6 w-6" strokeWidth={1.5} aria-hidden="true" />}>
      <p className="mx-auto mb-8 max-w-md text-lg text-muted-foreground">{intro}</p>
      <form onSubmit={submit} className="space-y-6 text-left">
        {children}
        <NewPasswordField value={password} onChange={setPassword} strength={strength} autoFocus={!children} />
        <Field
          label="Enter it again"
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          error={mismatch ? "The two passwords don't match." : null}
        />
        {error && (
          <p role="alert" className="text-base text-error">
            {error}
          </p>
        )}
        <div className="flex flex-col-reverse items-center justify-between gap-4 pt-2 sm:flex-row">
          {onBack ? (
            <Button variant="ghost" onClick={onBack} disabled={busy}>
              Back
            </Button>
          ) : (
            <span />
          )}
          <Button type="submit" disabled={!ready} className="w-full sm:w-auto">
            {busy ? busyLabel : submitLabel}
          </Button>
        </div>
      </form>
    </Frame>
  );
}
