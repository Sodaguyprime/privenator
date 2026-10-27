import { useEffect, useRef, useState } from "react";
import { activity, onLocked, passwordStrength, type Strength } from "./ipc";

/**
 * zxcvbn strength of `password`, scored in Rust after a short debounce.
 * Null while a score for the current value is pending, so callers never act
 * on a stale result.
 */
export function usePasswordStrength(password: string): Strength | null {
  const [result, setResult] = useState<{ for: string; strength: Strength } | null>(null);
  useEffect(() => {
    if (!password) return;
    const t = setTimeout(() => {
      passwordStrength(password).then((strength) => setResult({ for: password, strength }));
    }, 150);
    return () => clearTimeout(t);
  }, [password]);
  return result && result.for === password ? result.strength : null;
}

/** Whole seconds remaining until `deadline` (ms epoch), ticking down. */
export function useSecondsUntil(deadline: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (deadline <= Date.now()) return;
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, [deadline]);
  return Math.max(0, Math.ceil((deadline - now) / 1000));
}

/**
 * While `active`, tells Rust about keyboard/pointer input so the idle
 * auto-lock countdown restarts. At most one ping per 10 s; Rust decides when
 * to lock, so a missed ping only means locking a little early.
 */
export function useActivityPings(active: boolean) {
  useEffect(() => {
    if (!active) return;
    let last = 0;
    const ping = () => {
      const now = Date.now();
      if (now - last < 10_000) return;
      last = now;
      activity().catch(() => {});
    };
    const events = ["keydown", "pointerdown", "pointermove", "wheel"] as const;
    for (const e of events) window.addEventListener(e, ping, { passive: true });
    return () => {
      for (const e of events) window.removeEventListener(e, ping);
    };
  }, [active]);
}

/** Calls `handler` whenever Rust locks the journal (idle, minimize, or "Lock now"). */
export function useOnLocked(handler: () => void) {
  useEffect(() => {
    const unlisten = onLocked(handler);
    return () => {
      unlisten.then((f) => f());
    };
  }, [handler]);
}

/**
 * Subscribes to a Rust → UI event for the component's lifetime. The latest
 * `handler` is always used, without resubscribing.
 */
export function useTauriEvent<T>(subscribe: (h: (payload: T) => void) => Promise<() => void>, handler: (payload: T) => void) {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  useEffect(() => {
    const unlisten = subscribe((p) => latest.current(p));
    return () => {
      unlisten.then((f) => f());
    };
  }, [subscribe]);
}
