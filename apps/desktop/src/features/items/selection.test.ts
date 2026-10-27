import { describe, expect, it } from 'vitest';
import { clickSelect, prune, selectAll, selectedIds } from './selection';
const order = ['a', 'b', 'c', 'd', 'e'];
describe('item multi-select', () => {
  it('plain click selects one', () => {
    expect(clickSelect(order, { selected: 'a', multi: ['a', 'b'] }, 'c')).toEqual({ selected: 'c', multi: [] });
  });
  it('cmd-click toggles, keeps list order, collapses to single at one item', () => {
    let s = clickSelect(order, { selected: 'c', multi: [] }, 'a', { toggle: true });
    expect(s).toEqual({ selected: 'a', multi: ['a', 'c'] });
    s = clickSelect(order, s, 'e', { toggle: true });
    expect(s.multi).toEqual(['a', 'c', 'e']);
    s = clickSelect(order, s, 'c', { toggle: true });
    expect(s.multi).toEqual(['a', 'e']);
    s = clickSelect(order, s, 'a', { toggle: true });
    expect(s).toEqual({ selected: 'e', multi: [] });
    expect(clickSelect(order, s, 'e', { toggle: true })).toEqual({ selected: null, multi: [] });
  });
  it('shift-click selects a range from the anchor in either direction', () => {
    expect(clickSelect(order, { selected: 'b', multi: [] }, 'd', { range: true })).toEqual({ selected: 'b', multi: ['b', 'c', 'd'] });
    const s = clickSelect(order, { selected: 'b', multi: ['b', 'c', 'd'] }, 'a', { range: true });
    expect(s).toEqual({ selected: 'b', multi: ['a', 'b'] });
    // cmd+shift adds the range to the current selection
    expect(clickSelect(order, { selected: 'e', multi: ['a', 'e'] }, 'c', { range: true, toggle: true }).multi).toEqual(['a', 'c', 'd', 'e']);
    expect(clickSelect(order, { selected: 'c', multi: [] }, 'c', { range: true })).toEqual({ selected: 'c', multi: [] });
  });
  it('select all, prune and selectedIds', () => {
    const s = selectAll(order, { selected: 'c', multi: [] });
    expect(s).toEqual({ selected: 'c', multi: order });
    expect(selectAll(['x'], { selected: null, multi: [] })).toEqual({ selected: 'x', multi: [] });
    expect(prune(['a', 'b'], s)).toEqual({ selected: 'a', multi: ['a', 'b'] });
    expect(prune(['d'], s)).toEqual({ selected: 'd', multi: [] });
    expect(selectedIds({ selected: 'c', multi: [] })).toEqual(['c']);
    expect(selectedIds(s)).toEqual(order);
  });
});
