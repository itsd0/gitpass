import { describe, it, expect } from 'vitest';
import { api } from './client';
describe('mock api', () => {
  it('has ~40 items, 3 vaults incl. shared, and hides concealed values in details', async () => {
    const all = [...await api.listItems({ filter: { kind: 'all' } }), ...await api.listItems({ filter: { kind: 'archive' } }), ...await api.listItems({ filter: { kind: 'deleted' } })];
    expect(all.length).toBeGreaterThanOrEqual(38);
    const vaults = await api.listVaults(); expect(vaults).toHaveLength(3); expect(vaults.some((v) => v.shared)).toBe(true);
    for (const s of all.slice(0, 20)) {
      const d = await api.getItem({ vaultId: s.vaultId, itemId: s.id });
      d.sections.flatMap((x) => x.fields).filter((f) => f.kind === 'concealed' || f.kind === 'totp').forEach((f) => expect(f.value).toBeNull());
    }
  });
});
