// Pure multi-select logic for the item list (1Password-style): plain click selects one,
// Cmd/Ctrl-click toggles, Shift-click selects the range from the anchor, Cmd-A selects all.
import type { ItemSummary, ItemRef } from '../../api/types';
import { useApp } from '../../lib/store';

export interface Sel { selected: string | null; multi: string[] }
export interface Mods { toggle?: boolean; range?: boolean }

/** Ids currently selected (multi when >1, else the single selection). */
export const selectedIds = (s: Sel): string[] => (s.multi.length > 1 ? s.multi : s.selected ? [s.selected] : []);

export function clickSelect(order: string[], s: Sel, id: string, m: Mods = {}): Sel {
  const norm = (ids: string[], focus: string | null): Sel => {
    const set = new Set(ids); const out = order.filter((x) => set.has(x));
    if (out.length > 1) return { selected: focus && set.has(focus) ? focus : out[out.length - 1], multi: out };
    return { selected: out[0] ?? null, multi: [] };
  };
  if (m.range) {
    const anchor = s.selected && order.includes(s.selected) ? s.selected : id;
    const a = order.indexOf(anchor), b = order.indexOf(id);
    if (a < 0 || b < 0) return { selected: id, multi: [] };
    const span = order.slice(Math.min(a, b), Math.max(a, b) + 1);
    const base = m.toggle ? selectedIds(s) : [];
    const r = norm([...base, ...span], anchor);
    return r.multi.length ? { ...r, selected: anchor } : r;
  }
  if (m.toggle) {
    const cur = selectedIds(s);
    const next = cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id];
    return norm(next, cur.includes(id) ? s.selected : id);
  }
  return { selected: id, multi: [] };
}

export const selectAll = (order: string[], s: Sel): Sel =>
  order.length > 1 ? { selected: s.selected && order.includes(s.selected) ? s.selected : order[0], multi: [...order] } : { selected: order[0] ?? null, multi: [] };

/**
 * Arrow-key selection. Plain arrow returns to a single selection.
 * Shift+arrow moves the focus row and selects anchor..focus, following the
 * same convention as shift-click: `selected` is the anchor, `multi` is the
 * contiguous range in list order. Moving back toward the anchor shrinks the
 * range; moving past it expands it on the other side.
 *
 * The current focus is not stored separately. It is the endpoint opposite
 * the anchor only when `multi` is exactly a contiguous span in list order
 * and the anchor is one of its ends (see `rangeFocus`); in every other
 * case — Cmd-clicked, non-contiguous selections and Cmd-A, where the
 * anchor is not an end — the focus is `selected` itself, the last-clicked
 * item / anchor.
 */
function contiguousOppositeEnd(order: string[], s: Sel, anchor: string): string | null {
  if (s.multi.length <= 1) return null;
  const sorted = order.filter((x) => s.multi.includes(x));
  if (sorted.length !== s.multi.length) return null; // ids outside the list: not a range
  const first = order.indexOf(sorted[0]), last = order.indexOf(sorted[sorted.length - 1]);
  if (last - first + 1 !== sorted.length) return null; // gaps: not contiguous
  if (anchor !== sorted[0] && anchor !== sorted[sorted.length - 1]) return null; // anchor not at an end
  return anchor === sorted[0] ? sorted[sorted.length - 1] : sorted[0];
}

export function arrowSelect(order: string[], s: Sel, delta: 1 | -1, shift = false): Sel {
  if (!order.length) return s;
  const anchor = s.selected && order.includes(s.selected) ? s.selected : null;
  // Current focus: opposite endpoint of a contiguous range, else the anchor itself.
  const focusId = anchor ? contiguousOppositeEnd(order, s, anchor) ?? anchor : null;
  const focusIdx = focusId ? order.indexOf(focusId) : -1;
  const nextIdx = Math.max(0, Math.min(order.length - 1, focusIdx + delta));
  const nextId = order[nextIdx];
  if (!shift) return { selected: nextId, multi: [] };
  const rangeAnchor = anchor ?? nextId;
  const a = order.indexOf(rangeAnchor), b = nextIdx;
  const span = order.slice(Math.min(a, b), Math.max(a, b) + 1);
  return span.length > 1 ? { selected: rangeAnchor, multi: span } : { selected: rangeAnchor, multi: [] };
}

/** Focus row for a selection: the range endpoint opposite the anchor when
 *  `multi` is a contiguous span with the anchor at an end, else `selected`
 *  itself (Cmd-click / Cmd-A and other non-contiguous selections). */
export function rangeFocus(order: string[], s: Sel): string | null {
  if (s.selected && order.includes(s.selected) && s.multi.length > 1) {
    return contiguousOppositeEnd(order, s, s.selected) ?? s.selected;
  }
  return s.selected;
}

/** Keep only ids still present after a list refresh. */
export function prune(order: string[], s: Sel): Sel {
  const has = new Set(order);
  const multi = s.multi.filter((x) => has.has(x));
  if (multi.length > 1) return { selected: s.selected && has.has(s.selected) ? s.selected : multi[0], multi };
  if (s.multi.length) return { selected: multi[0] ?? null, multi: [] };
  return s;
}

/** Apply a Sel to the app store. */
export function applySel(s: Sel) { useApp.setState({ selectedItemId: s.selected, multiIds: s.multi, editing: false }); }
export const readSel = (): Sel => { const a = useApp.getState(); return { selected: a.selectedItemId, multi: a.multiIds }; };

/** Vault/item refs for the ids, in list order. */
export const refsFor = (items: ItemSummary[], ids: string[]): ItemRef[] => {
  const set = new Set(ids); return items.filter((x) => set.has(x.id)).map((x) => ({ vaultId: x.vaultId, itemId: x.id }));
};
