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
  it('supports version snapshot diffs and restoring previous versions', async () => {
    const item = (await api.listItems({ filter: { kind: 'all' } }))[0]!;
    const original = await api.getItem({ vaultId: item.vaultId, itemId: item.id });
    // Update the item
    await api.updateItem({
      itemId: item.id,
      input: {
        vaultId: item.vaultId,
        category: original.category,
        title: `${original.title} Edited`,
        sections: [
          ...original.sections,
          { id: 'custom-sec', title: 'Extra', fields: [{ id: 'f-custom', label: 'note', kind: 'text', value: 'custom value' }] },
        ],
      },
    });
    const updated = await api.getItem({ vaultId: item.vaultId, itemId: item.id });
    expect(updated.title).toBe(`${original.title} Edited`);
    expect(updated.history.length).toBeGreaterThanOrEqual(2);

    // View historical snapshot (version 1)
    const snap = await api.itemVersion({ vaultId: item.vaultId, itemId: item.id, version: 1 });
    expect(snap.title).toBe(original.title);
    // Added field in current item is marked as "added"
    expect(snap.sections.flatMap((s) => s.fields).some((f) => f.id === 'f-custom' && f.diff === 'added')).toBe(true);

    // Restore version 1
    const restored = await api.restoreItemVersion({ vaultId: item.vaultId, itemId: item.id, version: 1 });
    expect(restored.title).toBe(original.title);
    expect(restored.sections.flatMap((s) => s.fields).some((f) => f.id === 'f-custom')).toBe(false);
  });
});
