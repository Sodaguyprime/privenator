/** Microphone level: 24 brass segments, filled by `level` (0..1). */
export function LevelMeter({ level, label = "Microphone level" }: { level: number; label?: string }) {
  const lit = Math.round(level * 24);
  return (
    <div role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(level * 100)} className="flex h-6 items-end gap-1">
      {Array.from({ length: 24 }, (_, i) => (
        <span
          key={i}
          className={`w-1.5 rounded-full transition-colors duration-(--duration-fast) ${i < lit ? "bg-accent" : "bg-muted"}`}
          style={{ height: `${30 + (i / 23) * 70}%` }}
        />
      ))}
    </div>
  );
}
