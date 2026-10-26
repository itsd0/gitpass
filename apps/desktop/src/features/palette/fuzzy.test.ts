import { describe, expect, it } from 'vitest';
import { fuzzyScore, rank } from './fuzzy';
import { matches } from './shortcuts';
describe('fuzzy ranking', () => {
  it('ranks exact > prefix > substring > subsequence', () => {
    const items = ['Go to Settings', 'settings', 'Settings sync', 'xsetxtings'];
    expect(rank('settings', items, (x) => x)).toEqual(['settings', 'Settings sync', 'Go to Settings', 'xsetxtings']);
  });
  it('rejects non-matches', () => { expect(fuzzyScore('zzz', 'Example Bank')).toBeNull(); expect(rank('qq', ['a', 'b'], (x) => x)).toEqual([]); });
  it('prefers word-boundary acronyms', () => {
    expect(rank('gw', ['Go to Watchtower', 'glowworm extra long title'], (x) => x)[0]).toBe('Go to Watchtower');
  });
  it('uses best of multiple fields and stable order on ties', () => {
    const r = rank('bank', [{ t: 'Zeta', s: 'my bank login' }, { t: 'Alpha', s: 'bank' }, { t: 'Bank', s: '' }], (x) => [x.t, x.s]).map((x) => x.t);
    expect(r).toEqual(['Bank', 'Alpha', 'Zeta']); // primary-title match beats secondary exact match
  });
});
describe('palette command ranking', () => {
  it('title match beats keyword match ("se" → Settings before Watchtower[security])', () => {
    const cmds = [{ l: 'Go to Watchtower', k: ['security'] }, { l: 'Go to Settings', k: ['preferences'] }];
    expect(rank('se', cmds, (c) => [c.l, ...c.k])[0]!.l).toBe('Go to Settings');
  });
});
describe('shortcut matching', () => {
  const ev = (k: Partial<KeyboardEvent>) => ({ key: '', metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...k });
  it('mod maps to ctrl off-mac and meta on mac', () => {
    expect(matches('mod+k', ev({ key: 'k', ctrlKey: true }), false)).toBe(true);
    expect(matches('mod+k', ev({ key: 'k', metaKey: true }), true)).toBe(true);
    expect(matches('mod+k', ev({ key: 'k' }), false)).toBe(false);
    expect(matches('?', ev({ key: '?', shiftKey: true }), false)).toBe(true);
  });
});
