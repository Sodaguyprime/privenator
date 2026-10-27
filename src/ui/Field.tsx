import { useId, type InputHTMLAttributes } from "react";
import type { Strength } from "../lib/ipc";

const inputClass =
  "h-12 w-full rounded-default border border-border bg-background-alt px-4 font-body text-lg text-foreground " +
  "placeholder:italic placeholder:text-muted-foreground " +
  "transition-[border-color,box-shadow] duration-(--duration-base) ease-(--ease-classical) " +
  "focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30 " +
  "aria-[invalid=true]:border-accent-secondary";

type FieldProps = InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  /** Shown under the field; announced to screen readers. */
  hint?: string | null;
  error?: string | null;
};

/**
 * Labelled text input. Spellcheck and autocorrect are off everywhere: OS
 * spelling services can cache what they see (docs/SPEC.md "Rules").
 */
export function Field({ label, hint, error, className = "", ...rest }: FieldProps) {
  const id = useId();
  const describedBy = error || hint ? `${id}-desc` : undefined;
  return (
    <div className={`text-left ${className}`}>
      <label htmlFor={id} className="mb-2 block font-display text-[0.6875rem] font-medium uppercase tracking-[0.2em] text-muted-foreground">
        {label}
      </label>
      <input
        id={id}
        className={inputClass}
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
        autoComplete="off"
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        {...rest}
      />
      {(error || hint) && (
        <p id={describedBy} role={error ? "alert" : undefined} className={`mt-2 text-base ${error ? "text-error" : "text-muted-foreground"}`}>
          {error ?? hint}
        </p>
      )}
    </div>
  );
}

const SCORE_LABELS = ["Very weak", "Weak", "Fair", "Strong", "Very strong"];

/** A new-password field with the strength meter. */
export function NewPasswordField({
  value,
  onChange,
  strength,
  label = "New password",
  autoFocus,
}: {
  value: string;
  onChange: (v: string) => void;
  strength: Strength | null;
  label?: string;
  autoFocus?: boolean;
}) {
  return (
    <div>
      <Field label={label} type="password" value={value} autoFocus={autoFocus} onChange={(e) => onChange(e.target.value)} />
      <div className="mt-3 flex gap-1.5" aria-hidden="true">
        {[0, 1, 2, 3].map((i) => (
          <span
            key={i}
            className={`h-1 flex-1 rounded-full transition-colors duration-(--duration-base) ${
              strength && strength.score > i ? (strength.acceptable ? "bg-accent" : "bg-accent-secondary") : "bg-muted"
            }`}
          />
        ))}
      </div>
      <p className="mt-2 min-h-6 text-left text-base text-muted-foreground" aria-live="polite">
        {strength && (
          <>
            <span className={strength.acceptable ? "text-accent" : "text-error"}>{SCORE_LABELS[strength.score]}.</span>{" "}
            {strength.feedback}
          </>
        )}
      </p>
    </div>
  );
}
