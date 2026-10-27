import { X } from "lucide-react";
import { useId, useState } from "react";

/**
 * Free-text tags as chips. Enter or comma adds; suggestions come from the
 * tags already in use. Tags are lowercased and trimmed (docs/SPEC.md "Entries").
 */
export function TagInput({ value, onChange, suggestions }: { value: string[]; onChange: (tags: string[]) => void; suggestions: string[] }) {
  const id = useId();
  const [draft, setDraft] = useState("");

  function add(raw: string) {
    const tag = raw.trim().toLowerCase();
    setDraft("");
    if (tag && !value.includes(tag)) onChange([...value, tag].sort());
  }

  return (
    <div className="text-left">
      <label htmlFor={id} className="mb-2 block font-display text-[0.6875rem] font-medium uppercase tracking-[0.2em] text-muted-foreground">
        Tags
      </label>
      <div className="flex min-h-12 flex-wrap items-center gap-2 rounded-default border border-border bg-background-alt px-3 py-2 focus-within:border-accent">
        {value.map((tag) => (
          <span key={tag} className="inline-flex items-center gap-1 rounded-default border border-accent/40 px-2 py-0.5 font-body text-base text-accent">
            {tag}
            <button type="button" aria-label={`Remove tag ${tag}`} onClick={() => onChange(value.filter((t) => t !== tag))} className="text-muted-foreground hover:text-foreground">
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </span>
        ))}
        <input
          id={id}
          list={`${id}-list`}
          value={draft}
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          placeholder={value.length ? "" : "Add a tag…"}
          className="min-w-24 flex-1 bg-transparent font-body text-lg text-foreground placeholder:italic placeholder:text-muted-foreground focus:outline-none"
          onChange={(e) => (e.target.value.endsWith(",") ? add(e.target.value.slice(0, -1)) : setDraft(e.target.value))}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add(draft);
            } else if (e.key === "Backspace" && !draft && value.length) {
              onChange(value.slice(0, -1));
            }
          }}
          onBlur={() => add(draft)}
        />
        <datalist id={`${id}-list`}>
          {suggestions.filter((s) => !value.includes(s)).map((s) => (
            <option key={s} value={s} />
          ))}
        </datalist>
      </div>
    </div>
  );
}
