/** Ranges are half-open `[start, end)` offsets into the original string. */
export interface MatchRange {
  start: number;
  end: number;
}

export interface HighlightSegment {
  text: string;
  match: boolean;
}

/** Cap the number of marks so a very broad query stays cheap to render. */
const MAX_RANGES = 100;
const COMBINING_MARKS = /\p{M}/gu;

/**
 * Case- and diacritic-insensitive comparison key plus a map from every code
 * unit in the key back to the original character's `[start, end)` offsets.
 * One key entry per UTF-16 code unit keeps `indexOf` results aligned with
 * `starts` / `ends`.
 */
function fold(text: string): { key: string; starts: number[]; ends: number[] } {
  let key = "";
  const starts: number[] = [];
  const ends: number[] = [];
  let offset = 0;
  for (const ch of text) {
    const base = ch.normalize("NFD").replace(COMBINING_MARKS, "").toLowerCase();
    if (!base.length) {
      // A combining mark folds away, but it belongs to the preceding base
      // character: keep it inside that character's range so `<mark>` never
      // splits an accented glyph.
      if (ends.length) ends[ends.length - 1] = offset + ch.length;
      offset += ch.length;
      continue;
    }
    for (let i = 0; i < base.length; i++) {
      key += base[i];
      starts.push(offset);
      ends.push(offset + ch.length);
    }
    offset += ch.length;
  }
  return { key, starts, ends };
}

function foldQuery(query: string): string {
  return query.trim().normalize("NFD").replace(COMBINING_MARKS, "").toLowerCase();
}

/** Merge adjacent or overlapping ranges, preserving order. */
function merge(ranges: MatchRange[]): MatchRange[] {
  if (ranges.length < 2) return ranges;
  const out: MatchRange[] = [];
  for (const range of ranges) {
    const last = out[out.length - 1];
    if (last && range.start <= last.end) last.end = Math.max(last.end, range.end);
    else out.push({ ...range });
  }
  return out;
}

/**
 * Pure search highlighting: every occurrence of `query` in `text`, ignoring
 * case and diacritics. Falls back to a subsequence match ("ghl" → "GitHub
 * Login") when no contiguous occurrence exists.
 */
export function matchRanges(text: string, query: string): MatchRange[] {
  const q = foldQuery(query);
  if (!q || !text) return [];
  const { key, starts, ends } = fold(text);

  const ranges: MatchRange[] = [];
  let from = 0;
  while (ranges.length < MAX_RANGES) {
    const at = key.indexOf(q, from);
    if (at < 0) break;
    ranges.push({ start: starts[at], end: ends[at + q.length - 1] });
    from = at + q.length;
  }
  if (ranges.length) return merge(ranges);

  // Subsequence fallback: each query character in order, not necessarily
  // adjacent. Useful for initials and abbreviated searches.
  let at = 0;
  for (const ch of q) {
    at = key.indexOf(ch, at);
    if (at < 0) return [];
    ranges.push({ start: starts[at], end: ends[at] });
    at += ch.length;
  }
  return merge(ranges);
}

/** Split `text` into plain and matched segments for rendering. */
export function highlightSegments(text: string, query: string): HighlightSegment[] {
  const ranges = matchRanges(text, query);
  if (!ranges.length) return [{ text, match: false }];
  const segments: HighlightSegment[] = [];
  let pos = 0;
  for (const range of ranges) {
    if (range.start > pos) segments.push({ text: text.slice(pos, range.start), match: false });
    segments.push({ text: text.slice(range.start, range.end), match: true });
    pos = range.end;
  }
  if (pos < text.length) segments.push({ text: text.slice(pos), match: false });
  return segments;
}
