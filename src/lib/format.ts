// Dates, durations and labels as the journal shows them. Local time: the
// sidebar groups entries by the user's local date (docs/SPEC.md "Entries").

const dayFormat = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });

/** Grouping key for the sidebar: the local calendar date. */
export function dayKey(unixMs: number): string {
  const d = new Date(unixMs);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

export const formatDay = (unixMs: number) => dayFormat.format(unixMs);
export const formatTime = (unixMs: number) => timeFormat.format(unixMs);

/** 754 → "12:34"; an hour or more → "1:02:03". */
export function formatDuration(seconds: number): string {
  const s = Math.floor(seconds % 60);
  const m = Math.floor(seconds / 60) % 60;
  const h = Math.floor(seconds / 3600);
  const mm = h ? String(m).padStart(2, "0") : String(m);
  return `${h ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
}

/** The 5-point mood scale (docs/SPEC.md "Entries"). Index 0 = mood 1. */
export const MOODS = ["Very low", "Low", "Neutral", "Good", "Very good"] as const;

/** Start of the local day, for date-range filters (`<input type="date">` values). */
export function startOfDay(isoDate: string): number {
  const [y, m, d] = isoDate.split("-").map(Number);
  return new Date(y, m - 1, d).getTime();
}
