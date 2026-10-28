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
  it('empties only the trash and returns the purged count', async () => {
    const before = await api.listItems({ filter: { kind: 'deleted' } });
    expect(before.length).toBeGreaterThan(0);
    const active = (await api.listItems({ filter: { kind: 'all' } })).length;
    expect(await api.emptyTrash()).toBe(before.length);
    expect(await api.listItems({ filter: { kind: 'deleted' } })).toHaveLength(0);
    expect(await api.listItems({ filter: { kind: 'all' } })).toHaveLength(active);
    expect(await api.emptyTrash()).toBe(0);
  });
});
