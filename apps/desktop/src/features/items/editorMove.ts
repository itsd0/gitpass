/** Return a copy of `list` with the element at `from` moved to index `to` (clamped). */
export function moveItem<T>(list: readonly T[], from: number, to: number): T[] {
  const out = list.slice();
  if (from < 0 || from >= out.length) return out;
  const dest = Math.max(0, Math.min(out.length - 1, to));
  if (dest === from) return out;
  const [x] = out.splice(from, 1);
  out.splice(dest, 0, x);
  return out;
}

/** Insert `item` at `index` (clamped to the list length). */
export function insertAt<T>(list: readonly T[], index: number, item: T): T[] {
  const out = list.slice();
  out.splice(Math.max(0, Math.min(out.length, index)), 0, item);
  return out;
}
