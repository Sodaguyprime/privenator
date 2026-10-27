import { MOODS } from "../lib/format";

/** The 5-point mood scale. Choosing the selected mood again clears it. */
export function MoodPicker({ value, onChange }: { value: number | null; onChange: (mood: number | null) => void }) {
  return (
    <fieldset>
      <legend className="mb-2 font-display text-[0.6875rem] font-medium uppercase tracking-[0.2em] text-muted-foreground">Mood</legend>
      <div className="flex flex-wrap gap-2">
        {MOODS.map((label, i) => {
          const mood = i + 1;
          const selected = value === mood;
          return (
            <button
              key={mood}
              type="button"
              aria-pressed={selected}
              onClick={() => onChange(selected ? null : mood)}
              className={`h-10 rounded-default border px-3 font-body text-base transition-colors duration-(--duration-base) ${
                selected ? "border-accent bg-accent/15 text-accent" : "border-border text-muted-foreground hover:border-accent/60 hover:text-foreground"
              }`}
            >
              {label}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}
