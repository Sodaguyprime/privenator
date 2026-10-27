import { useCallback, useEffect, useState } from "react";
import { useActivityPings, useOnLocked } from "./lib/hooks";
import * as ipc from "./lib/ipc";
import { CreatePassword } from "./screens/CreatePassword";
import { Journal } from "./screens/Journal";
import { MicCheck } from "./screens/MicCheck";
import { RestoreForm } from "./screens/journal/SettingsPanel";
import { RecoveryKeySheet } from "./screens/RecoveryKeySheet";
import { Unlock } from "./screens/Unlock";
import { Welcome } from "./screens/Welcome";
import { Field } from "./ui/Field";
import { Frame } from "./ui/Frame";
import { ArchiveRestore } from "lucide-react";
import { Atmosphere } from "./ui/Ornaments";

/**
 * Screen flow (docs/SPEC.md "User flows"):
 *   no vault → welcome → create password → recovery key → mic check → journal
 *            → welcome → restore from backup → unlock
 *   locked   → unlock → journal
 *            → forgot → recovery key + new password → new recovery key → journal
 *   journal  → (Settings) restore → unlock
 * Any `locked` event from Rust returns to the unlock screen.
 */
type Screen =
  | { name: "loading" }
  | { name: "welcome" }
  | { name: "create" }
  | { name: "recovery-key"; key: string; replacing: boolean }
  | { name: "unlock" }
  | { name: "recover" }
  | { name: "mic-check" }
  | { name: "restore" }
  | { name: "journal"; recovered?: number };

const toScreen: Record<ipc.VaultStatus, Screen> = {
  none: { name: "welcome" },
  locked: { name: "unlock" },
  unlocked: { name: "journal" },
};

export default function App() {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });
  const [version, setVersion] = useState<string>();

  useEffect(() => {
    ipc.vaultStatus().then((s) => setScreen(toScreen[s]), () => setScreen({ name: "welcome" }));
    ipc.appInfo().then((info) => setVersion(info.app_version), () => {});
  }, []);

  // Rust locks (idle, minimize, "Lock now") and then emits `locked`. Leaving
  // the journal screen unmounts it, which drops any plaintext it held.
  const showUnlock = useCallback(() => setScreen({ name: "unlock" }), []);
  useOnLocked(showUnlock);
  useActivityPings(screen.name === "journal");

  const lock = useCallback(async () => {
    await ipc.lock();
    showUnlock();
  }, [showUnlock]);

  const keyConfirmed = useCallback(async (replacing: boolean) => {
    await ipc.releaseAutolock();
    setScreen(replacing ? { name: "journal" } : { name: "mic-check" });
  }, []);

  return (
    <>
      {screen.name === "welcome" && <Welcome version={version} onBegin={() => setScreen({ name: "create" })} onRestore={() => setScreen({ name: "restore" })} />}

      {screen.name === "create" && (
        <CreatePassword
          overline="Volume I"
          title="Choose a password"
          intro="It opens your journal on this device. Use at least 12 characters; a few unrelated words work well."
          submitLabel="Seal the journal"
          busyLabel="Sealing…"
          onSubmit={ipc.createVault}
          onDone={(key) => setScreen({ name: "recovery-key", key, replacing: false })}
          onBack={() => setScreen({ name: "welcome" })}
        />
      )}

      {screen.name === "recovery-key" && (
        <RecoveryKeySheet recoveryKey={screen.key} replacing={screen.replacing} onConfirmed={() => keyConfirmed(screen.replacing)} />
      )}

      {screen.name === "unlock" && <Unlock onUnlocked={(recovered) => setScreen({ name: "journal", recovered })} onForgot={() => setScreen({ name: "recover" })} />}

      {screen.name === "recover" && <Recover onDone={(key) => setScreen({ name: "recovery-key", key, replacing: true })} onBack={() => setScreen({ name: "unlock" })} />}

      {screen.name === "mic-check" && <MicCheck onDone={() => setScreen({ name: "journal" })} />}

      {screen.name === "restore" && (
        <Frame overline="Restore" title="Open a backup" titleId="restore-title" medallion={<ArchiveRestore className="h-6 w-6" strokeWidth={1.5} aria-hidden="true" />}>
          <p className="mx-auto mb-8 max-w-md text-lg text-muted-foreground">Choose a .vjbak file made by Privenator. Afterwards, unlock with the password the journal had when it was backed up.</p>
          <RestoreForm replacing={false} onRestored={showUnlock} />
          <button type="button" onClick={() => setScreen({ name: "welcome" })} className="mt-8 font-display text-xs uppercase tracking-[0.15em] text-accent">
            Back
          </button>
        </Frame>
      )}

      {screen.name === "journal" && <Journal recovered={screen.recovered ?? 0} onLock={lock} onRestored={showUnlock} />}

      <Atmosphere />
    </>
  );
}

function Recover({ onDone, onBack }: { onDone: (key: string) => void; onBack: () => void }) {
  const [key, setKey] = useState("");
  const complete = key.replace(/[\s-]/g, "").length === 26;

  async function submit(password: string) {
    try {
      return await ipc.recover(key, password);
    } catch (e) {
      const ce = ipc.asCommandError(e);
      if (ce.kind === "wrong_recovery_key" || ce.kind === "invalid_recovery_key")
        throw { ...ce, message: "That recovery key doesn't open this journal." };
      if (ce.kind === "throttled" && ce.retry_after_ms)
        throw { ...ce, message: `Too many attempts. Try again in ${Math.ceil(ce.retry_after_ms / 1000)} s.` };
      throw ce;
    }
  }

  return (
    <CreatePassword
      overline="Forgot password"
      title="Use your recovery key"
      intro="Enter the recovery key you kept, then choose a new password. You'll get a new recovery key afterwards."
      submitLabel="Set new password"
      busyLabel="Opening…"
      onSubmit={submit}
      onDone={onDone}
      onBack={onBack}
      canSubmit={complete}
    >
      <Field
        label="Recovery key"
        value={key}
        autoFocus
        placeholder="XXXX-XXXX-XXXX-…"
        className="[&_input]:font-display [&_input]:tracking-[0.15em] [&_input]:uppercase"
        onChange={(e) => setKey(e.target.value)}
      />
    </CreatePassword>
  );
}
