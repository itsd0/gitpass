import { describe, expect, it } from 'vitest';
import { insertAt, moveItem } from './editorMove';

describe('editorMove', () => {
  it('moves forward, backward and clamps', () => {
    expect(moveItem(['a', 'b', 'c'], 0, 2)).toEqual(['b', 'c', 'a']);
    expect(moveItem(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b']);
    expect(moveItem(['a', 'b', 'c'], 0, -1)).toEqual(['a', 'b', 'c']);
    expect(moveItem(['a', 'b', 'c'], 1, 9)).toEqual(['a', 'c', 'b']);
    expect(moveItem(['a'], 3, 0)).toEqual(['a']);
  });
  it('does not mutate the input', () => {
    const src = ['a', 'b'];
    moveItem(src, 0, 1); insertAt(src, 0, 'z');
    expect(src).toEqual(['a', 'b']);
  });
  it('inserts at an index, clamped', () => {
    expect(insertAt(['a', 'c'], 1, 'b')).toEqual(['a', 'b', 'c']);
    expect(insertAt(['a'], 9, 'z')).toEqual(['a', 'z']);
  });
});
