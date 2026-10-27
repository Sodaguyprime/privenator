import { ListFilter, Search, X } from "lucide-react";
import { Fragment, useEffect, useState } from "react";
import { dayKey, formatDay, formatDuration, formatTime, MOODS, startOfDay } from "../../lib/format";
import * as ipc from "../../lib/ipc";
import { toRoman } from "../../lib/roman";
import { Divider } from "../../ui/Ornaments";

type Filters = { mood?: number; tag?: string; from?: string; to?: string };
const GLYPHS = ["✶", "❧", "✤", "❦"] as const;

/**
 * Entries by local day, newest first, with search and filters
 * (docs/SPEC.md "Entries", "Search and backup").
 */
export function Sidebar({
  version,
  selectedId,
  onSelect,
}: {
  /** Bump to reload the list after a change. */
  version: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const [text, setText] = useState("");
  const [filters, setFilters] = useState<Filters>({});
  const [showFilters, setShowFilters] = useState(false);
  const [hits, setHits] = useState<ipc.SearchHit[]>([]);
  const [tags, setTags] = useState<string[]>([]);
  const searching = !!text.trim() || Object.values(filters).some((v) => v !== undefined && v !== "");

  useEffect(() => {
    ipc.allTags().then(setTags);
  }, [version]);

  useEffect(() => {
    let current = true;
    const query: ipc.SearchQuery = {
      text,
      mood: filters.mood,
      tag: filters.tag || undefined,
      from: filters.from ? startOfDay(filters.from) : undefined,
      to: filters.to ? startOfDay(filters.to) + 86_400_000 : undefined,
    };
    const t = setTimeout(() => ipc.searchEntries(query).then((h) => current && setHits(h)), text ? 150 : 0);
    return () => {
      current = false;
      clearTimeout(t);
    };
  }, [text, filters, version]);

  // Group by local day; number entries within a day oldest-first (I, II, III).
  const days: { key: string; at: number; items: { hit: ipc.SearchHit; n: number }[] }[] = [];
  for (const hit of hits) {
    const key = dayKey(hit.entry.created_at);
    let day = days.find((d) => d.key === key);
    if (!day) {
      day = { key, at: hit.entry.created_at, items: [] };
      days.push(day);
    }
    day.items.push({ hit, n: 0 });
  }
  for (const day of days) day.items.forEach((item, i) => (item.n = day.items.length - i));

  const label = "mb-1 block font-display text-[0.625rem] uppercase tracking-[0.2em] text-muted-foreground";
  const control = "h-10 w-full rounded-default border border-border bg-background-alt px-2 text-foreground";

  return (
    <div className="flex h-full flex-col">
      <div className="space-y-3 border-b border-border p-4">
        <div className="flex items-center gap-2 rounded-default border border-border bg-background-alt px-3 focus-within:border-accent">
          <Search className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <input
            type="search"
            aria-label="Search entries"
            placeholder="Search the journal…"
            value={text}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setText(e.target.value)}
            className="h-10 w-full bg-transparent font-body text-lg text-foreground placeholder:italic placeholder:text-muted-foreground focus:outline-none [&::-webkit-search-cancel-button]:appearance-none"
          />
          <button
            type="button"
            aria-label="Filters"
            aria-expanded={showFilters}
            onClick={() => setShowFilters((v) => !v)}
            className={`grid h-8 w-8 place-items-center rounded-default ${showFilters ? "text-accent" : "text-muted-foreground hover:text-foreground"}`}
          >
            <ListFilter className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        {showFilters && (
          <div className="grid grid-cols-2 gap-2 font-body text-base">
            <label>
              <span className={label}>Mood</span>
              <select value={filters.mood ?? ""} onChange={(e) => setFilters({ ...filters, mood: e.target.value ? Number(e.target.value) : undefined })} className={control}>
                <option value="">Any</option>
                {MOODS.map((m, i) => (
                  <option key={m} value={i + 1}>
                    {m}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span className={label}>Tag</span>
              <select value={filters.tag ?? ""} onChange={(e) => setFilters({ ...filters, tag: e.target.value || undefined })} className={control}>
                <option value="">Any</option>
                {tags.map((t) => (
                  <option key={t}>{t}</option>
                ))}
              </select>
            </label>
            <label>
              <span className={label}>From</span>
              <input type="date" value={filters.from ?? ""} onChange={(e) => setFilters({ ...filters, from: e.target.value || undefined })} className={`${control} [color-scheme:dark]`} />
            </label>
            <label>
              <span className={label}>To</span>
              <input type="date" value={filters.to ?? ""} onChange={(e) => setFilters({ ...filters, to: e.target.value || undefined })} className={`${control} [color-scheme:dark]`} />
            </label>
            {searching && (
              <button
                type="button"
                onClick={() => {
                  setFilters({});
                  setText("");
                }}
                className="col-span-2 inline-flex items-center gap-1 justify-self-start font-display text-xs uppercase tracking-[0.15em] text-accent hover:text-accent-bright"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" /> Clear
              </button>
            )}
          </div>
        )}
      </div>

      <nav aria-label="Entries" className="flex-1 overflow-y-auto px-2 py-4">
        {!hits.length && (
          <p className="px-4 py-8 text-center text-lg italic text-muted-foreground">{searching ? "Nothing matches." : "No entries yet. Press record to begin."}</p>
        )}
        {days.map((day, d) => (
          <Fragment key={day.key}>
            {d > 0 && <Divider glyph={GLYPHS[d % GLYPHS.length]} className="mx-auto my-4 w-32" />}
            <h2 className="inscription px-3 pb-2 text-muted-foreground">{formatDay(day.at)}</h2>
            <ul>
              {day.items.map(({ hit, n }) => (
                <li key={hit.entry.id}>
                  <button
                    type="button"
                    aria-current={selectedId === hit.entry.id ? "true" : undefined}
                    onClick={() => onSelect(hit.entry.id)}
                    className={`w-full rounded-default border px-3 py-2 text-left transition-colors duration-(--duration-base) ${
                      selectedId === hit.entry.id ? "border-accent/50 bg-accent/10" : "border-transparent hover:bg-background-alt"
                    }`}
                  >
                    <span className="flex items-baseline gap-2">
                      <span className="font-display text-xs text-accent">{toRoman(n)}</span>
                      <span className="truncate font-heading text-xl text-foreground">{hit.entry.title ?? "Untitled"}</span>
                    </span>
                    <span className="mt-0.5 block font-body text-sm text-muted-foreground">
                      {formatTime(hit.entry.created_at)}
                      {hit.entry.duration_s > 0 && ` · ${formatDuration(hit.entry.duration_s)}`}
                      {hit.entry.mood && ` · ${MOODS[hit.entry.mood - 1]}`}
                    </span>
                    {hit.snippet && <Snippet text={hit.snippet} />}
                  </button>
                </li>
              ))}
            </ul>
          </Fragment>
        ))}
      </nav>
    </div>
  );
}

/** Renders the U+E000…U+E001 match markers as highlights, without innerHTML. */
function Snippet({ text }: { text: string }) {
  const parts = text.split(/([^]*)/);
  return (
    <span className="mt-1 line-clamp-2 block font-body text-sm text-muted-foreground">
      {parts.map((p, i) =>
        p.startsWith("") ? (
          <mark key={i} className="rounded-sm bg-accent/25 px-0.5 text-foreground">
            {p.slice(1, -1)}
          </mark>
        ) : (
          <Fragment key={i}>{p}</Fragment>
        ),
      )}
    </span>
  );
}
