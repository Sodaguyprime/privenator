import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { formatDay } from "../../lib/format";
import { usePasswordStrength } from "../../lib/hooks";
import * as ipc from "../../lib/ipc";
import { Button } from "../../ui/Button";
import { Field, NewPasswordField } from "../../ui/Field";
import { Divider, VolumeLabel } from "../../ui/Ornaments";
import { MicTester } from "../MicCheck";
import { RecoveryKeySheet } from "../RecoveryKeySheet";

/** Settings (docs/SPEC.md "Settings"). */
export function SettingsPanel({ onRestored }: { onRestored: () => void }) {
  const [settings, setSettings] = useState<ipc.Settings | null>(null);
  const [newKey, setNewKey] = useState<string | null>(null);

  useEffect(() => {
    ipc.getSettings().then(setSettings);
  }, []);

  async function update(u: ipc.SettingsUpdate) {
    setSettings(await ipc.updateSettings(u));
  }

  if (newKey) {
    return (
      <RecoveryKeySheet
        recoveryKey={newKey}
        replacing
        onConfirmed={async () => {
          await ipc.releaseAutolock();
          setNewKey(null);
        }}
      />
    );
  }
  if (!settings) return null;

  return (
    <section aria-labelledby="settings-title" className="mx-auto max-w-2xl px-6 py-10">
      <div className="text-center">
        <VolumeLabel>Settings</VolumeLabel>
        <h1 id="settings-title" className="mt-5 text-4xl tracking-tight">
          The keeper's desk
        </h1>
        <Divider glyph="❧" className="mx-auto my-8 w-56" />
      </div>

      <Section title="Locking">
        <label className="block text-left">
          <span className="mb-2 block text-lg">Lock after this long without input</span>
          <select
            value={settings.idle_timeout_min}
            onChange={(e) => update({ idle_timeout_min: Number(e.target.value) })}
            className="h-12 rounded-default border border-border bg-background-alt px-3 font-body text-lg text-foreground focus:border-accent focus:outline-none"
          >
            {[1, 5, 15, 30].map((m) => (
              <option key={m} value={m}>
                {m} minute{m > 1 ? "s" : ""}
              </option>
            ))}
          </select>
        </label>
        <p className="text-base text-muted-foreground">The journal also locks whenever the window is minimized.</p>
      </Section>

      <Section title="Recording">
        <MicTester />
        <label className="flex cursor-pointer items-start gap-3 text-lg">
          <input type="checkbox" className="mt-1.5 h-4 w-4 accent-(--color-accent)" checked={settings.default_keep_audio} onChange={(e) => update({ default_keep_audio: e.target.checked })} />
          <span>
            Keep audio by default
            <span className="block text-base text-muted-foreground">You can still choose for each entry when you save it.</span>
          </span>
        </label>
      </Section>

      <Section title="Password">
        <ChangePassword />
      </Section>

      <Section title="Recovery key">
        <p className="text-base text-muted-foreground">Make a new recovery key if the old one may have been seen. The old key stops working.</p>
        <PasswordAction label="Make a new recovery key" busyLabel="Making…" run={async (pw) => setNewKey(await ipc.regenerateRecoveryKey(pw))} />
      </Section>

      <Section title="Backup">
        <BackupForm onDone={async () => setSettings(await ipc.getSettings())} />
        <p className="text-base text-muted-foreground">{settings.last_backup_at ? `Last backup: ${formatDay(settings.last_backup_at)}.` : "No backup made yet."}</p>
        <label className="block text-left">
          <span className="mb-2 block text-lg">Remind me to back up</span>
          <select
            value={settings.backup_reminder_days ?? ""}
            onChange={(e) => update({ backup_reminder_days: e.target.value ? Number(e.target.value) : null })}
            className="h-12 rounded-default border border-border bg-background-alt px-3 font-body text-lg text-foreground focus:border-accent focus:outline-none"
          >
            <option value="">Never</option>
            {[7, 14, 30, 90].map((d) => (
              <option key={d} value={d}>
                Every {d} days
              </option>
            ))}
          </select>
        </label>
      </Section>

      <Section title="Restore">
        <RestoreForm replacing onRestored={onRestored} />
      </Section>
    </section>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-4 border-t border-border py-8">
      <h2 className="font-display text-sm uppercase tracking-[0.2em] text-accent">{title}</h2>
      {children}
    </section>
  );
}

function ChangePassword() {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const strength = usePasswordStrength(next);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const ready = !!current && !!strength?.acceptable && next === confirm && !busy;

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    try {
      await ipc.changePassword(current, next);
      setCurrent("");
      setNext("");
      setConfirm("");
      setStatus({ ok: true, text: "Password changed. Your recovery key still works." });
    } catch (err) {
      const ce = ipc.asCommandError(err);
      setStatus({ ok: false, text: ce.kind === "wrong_password" ? "The current password is wrong." : ce.message });
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="space-y-4">
      <Field label="Current password" type="password" value={current} onChange={(e) => setCurrent(e.target.value)} />
      <NewPasswordField value={next} onChange={setNext} strength={strength} />
      <Field label="New password, again" type="password" value={confirm} error={confirm && confirm !== next ? "The passwords don't match." : null} onChange={(e) => setConfirm(e.target.value)} />
      <Button type="submit" size="sm" disabled={!ready}>
        {busy ? "Changing…" : "Change password"}
      </Button>
      {status && <p className={`text-base ${status.ok ? "text-accent" : "text-error"}`}>{status.text}</p>}
    </form>
  );
}

/** A single "current password" field guarding an action. */
function PasswordAction({ label, busyLabel, run }: { label: string; busyLabel: string; run: (password: string) => Promise<void> }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(undefined);
    try {
      await run(password);
    } catch (err) {
      const ce = ipc.asCommandError(err);
      setError(ce.kind === "wrong_password" ? "The password is wrong." : ce.message);
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="space-y-4">
      <Field label="Current password" type="password" value={password} error={error} onChange={(e) => setPassword(e.target.value)} />
      <Button type="submit" size="sm" variant="secondary" disabled={!password || busy}>
        {busy ? busyLabel : label}
      </Button>
    </form>
  );
}

function BackupForm({ onDone }: { onDone: () => void }) {
  const [usePassword, setUsePassword] = useState(true);
  const [secret, setSecret] = useState("");
  const [confirm, setConfirm] = useState("");
  const strength = usePasswordStrength(usePassword ? "" : secret);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const ready = !busy && !!secret && (usePassword || (!!strength?.acceptable && secret === confirm));

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    setStatus(null);
    try {
      if (await ipc.createBackup(usePassword, secret)) {
        setSecret("");
        setConfirm("");
        setStatus({ ok: true, text: "Backup saved." });
        onDone();
      }
    } catch (err) {
      const ce = ipc.asCommandError(err);
      setStatus({ ok: false, text: ce.kind === "wrong_password" ? "That isn't your journal password." : ce.message });
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="space-y-4">
      <p className="text-base text-muted-foreground">
        One encrypted file with every entry and kept recording. Save it somewhere other than this computer, such as a USB drive.
      </p>
      <fieldset className="space-y-2 text-lg">
        <legend className="sr-only">Backup passphrase</legend>
        <label className="flex cursor-pointer items-center gap-3">
          <input type="radio" name="backup-pass" checked={usePassword} onChange={() => setUsePassword(true)} className="accent-(--color-accent)" />
          Use my journal password
        </label>
        <label className="flex cursor-pointer items-center gap-3">
          <input type="radio" name="backup-pass" checked={!usePassword} onChange={() => setUsePassword(false)} className="accent-(--color-accent)" />
          Use a separate passphrase
        </label>
      </fieldset>
      {usePassword ? (
        <>
          <p className="text-base italic text-muted-foreground">Anyone who knows your password can then open the backup too.</p>
          <Field label="Journal password" type="password" value={secret} onChange={(e) => setSecret(e.target.value)} />
        </>
      ) : (
        <>
          <NewPasswordField label="Backup passphrase" value={secret} onChange={setSecret} strength={strength} />
          <Field label="Passphrase, again" type="password" value={confirm} error={confirm && confirm !== secret ? "The passphrases don't match." : null} onChange={(e) => setConfirm(e.target.value)} />
          <p className="text-base italic text-muted-foreground">Without this passphrase the backup can't be opened by anyone.</p>
        </>
      )}
      <Button type="submit" size="sm" disabled={!ready}>
        {busy ? "Sealing the backup…" : "Back up now"}
      </Button>
      {status && <p className={`text-base ${status.ok ? "text-accent" : "text-error"}`}>{status.text}</p>}
    </form>
  );
}

/**
 * Restore from a `.vjbak`. When a journal already exists, a typed
 * confirmation guards it, since restoring replaces everything.
 */
export function RestoreForm({ replacing, onRestored }: { replacing: boolean; onRestored: () => void }) {
  const [passphrase, setPassphrase] = useState("");
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const ready = !!passphrase && (!replacing || typed.trim().toUpperCase() === "REPLACE") && !busy;

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    setError(undefined);
    try {
      if (await ipc.restoreBackup(passphrase)) onRestored();
      else setBusy(false);
    } catch (err) {
      const ce = ipc.asCommandError(err);
      setError(ce.kind === "wrong_passphrase" ? "That passphrase doesn't open this backup, or the file is damaged." : ce.message);
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="space-y-4 text-left">
      {replacing && (
        <p className="text-base text-error">Restoring replaces this journal, every entry and recording in it, with the backup's. The backup's password applies afterwards.</p>
      )}
      <Field label="Backup passphrase" type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} hint="Usually the journal password from when the backup was made." />
      {replacing && <Field label='Type "replace" to confirm' value={typed} onChange={(e) => setTyped(e.target.value)} />}
      {error && (
        <p role="alert" className="text-base text-error">
          {error}
        </p>
      )}
      <Button type="submit" size="sm" variant="secondary" disabled={!ready}>
        {busy ? "Restoring…" : "Choose backup file…"}
      </Button>
    </form>
  );
}
