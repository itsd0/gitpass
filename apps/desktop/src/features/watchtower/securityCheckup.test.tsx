// Security checkup (issue #401): queue ordering, per-finding actions, keyboard,
// score delta and the end summary. All API calls mocked; no real vault.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../api/client';
import { useSettings } from '../settings/useSettings';
import { useI18n } from '../../i18n';
import { useToasts } from '../../ui';
import { useApp } from '../../lib/store';
import { buildCheckupQueue } from './checkup';

// Stub the generator popover: it needs TooltipProvider and its own suite covers it.
let stubbedPw: string | null = null;
vi.mock('../new-item/PasswordGenerator', () => ({
  PasswordGenerator: ({ open, onUse }: { open?: boolean; onUse?: (pw: string) => void }) =>
    open ? (
      <button type="button" data-testid="generator-stub" onClick={() => onUse && onUse(stubbedPw ?? 'Stub-Passw0rd!')}>
        use generated
      </button>
    ) : null,
  ColoredPassword: () => null,
  StrengthMeter: () => null,
}));
import { SecurityCheckup } from './SecurityCheckup';

const mk = (id: string, extra: Record<string, unknown> = {}) => ({
  id, vaultId: 'v1', category: 'login' as const, title: `Item ${id}`, subtitle: `user-${id}`,
  favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '', createdAt: '',
  url: `https://${id}.example.com`, changePasswordUrl: `https://${id}.example.com/.well-known/change-password`,
  ...extra,
});
const report = (over: Record<string, unknown> = {}) => ({
  score: 72, weak: ['w1'], reused: [['r1', 'r2']], old: ['o1'], breached: ['b1'],
  missingTotp: ['m1'], totpAvailable: [], passkeyAvailable: [], expired: [], expiringSoon: [],
  insecureUrls: [], duplicates: [], ignored: [], checkedAt: '', hibpChecked: true, ...over,
});

beforeEach(() => {
  useSettings.setState({ settings: { autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: '', hibpCheck: true } });
  vi.spyOn(api, 'watchtower').mockResolvedValue(report() as never);
  vi.spyOn(api, 'listItems').mockResolvedValue(['b1', 'r1', 'r2', 'w1', 'o1', 'm1'].map((id) => mk(id)) as never);
  vi.spyOn(api, 'ignoreFinding').mockResolvedValue(undefined);
  vi.spyOn(api, 'openChangePassword').mockResolvedValue(undefined);
  vi.spyOn(api, 'getItem').mockImplementation(async (a: { vaultId: string; itemId: string }) => ({
    ...mk(a.itemId), sections: [{ id: 's1', title: null, fields: [{ id: 'f1', label: 'Password', kind: 'concealed', value: null }] }],
    notes: null, attachments: [], history: [], hasTotp: false,
  }) as never);
  vi.spyOn(api, 'updateItem').mockResolvedValue(mk('x') as never);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); useI18n.getState().setLang('en'); useToasts.setState({ list: [] }); });

describe('buildCheckupQueue', () => {
  it('orders breached, reused, weak, old/rotation, missing 2FA', () => {
    const q = buildCheckupQueue(report({ rotationDue: [{ itemId: 'rot1', days: 400 }], rotationSoon: [{ itemId: 'rot2', days: 350 }] }) as never);
    expect(q.map((f) => f.itemId)).toEqual(['b1', 'r1', 'r2', 'w1', 'o1', 'rot1', 'rot2', 'm1']);
    expect(q.map((f) => f.kind)).toEqual(['breached', 'reused', 'reused', 'weak', 'old', 'old', 'old', 'missingTotp']);
  });
  it('excludes ignored findings and known-2FA sites', () => {
    const q = buildCheckupQueue(report({
      ignored: [{ itemId: 'w1', vaultId: 'v1', title: 'x', kind: 'weak', description: '' }],
      totpAvailable: ['m1'],
    }) as never);
    expect(q.map((f) => f.itemId)).not.toContain('w1');
    expect(q.map((f) => f.itemId)).not.toContain('m1');
  });
  it('maps finding kinds to ignore_finding kinds', () => {
    const q = buildCheckupQueue(report() as never);
    expect(q.find((f) => f.kind === 'breached')?.ignoreKind).toBe('pwned');
    expect(q.find((f) => f.kind === 'missingTotp')?.ignoreKind).toBe('missing_totp');
  });
});

describe('SecurityCheckup modal', () => {
  it('shows the first finding by impact with progress and item details', async () => {
    render(<SecurityCheckup onClose={() => {}} />);
    await screen.findByTestId('checkup-step');
    expect(screen.getByTestId('checkup-progress').textContent).toBe('1 of 6');
    expect(screen.getByTestId('checkup-item-title').textContent).toBe('Item b1');
    expect(screen.getByTestId('checkup-explanation').textContent).toContain('data breach');
  });

  it('Ignore calls ignore_finding with the mapped kind and advances', async () => {
    render(<SecurityCheckup onClose={() => {}} />);
    await screen.findByTestId('checkup-step');
    fireEvent.click(screen.getByTestId('checkup-ignore'));
    await waitFor(() => expect(api.ignoreFinding).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'b1', kind: 'pwned' }));
    await waitFor(() => expect(screen.getByTestId('checkup-progress').textContent).toBe('2 of 6'));
  });

  it('Generate & update saves the new password via the item update API', async () => {
    render(<SecurityCheckup onClose={() => {}} />);
    await screen.findByTestId('checkup-step');
    fireEvent.click(screen.getByTestId('checkup-generate-update'));
    fireEvent.click(await screen.findByTestId('generator-stub'));
    await waitFor(() => expect(api.updateItem).toHaveBeenCalled(), { timeout: 3000 });
    const call = vi.mocked(api.updateItem).mock.calls[0][0] as { itemId: string; input: { sections: { fields: { kind: string; value: string | null }[] }[] } };
    expect(call.itemId).toBe('b1');
    const pw = call.input.sections[0].fields[0].value;
    expect(typeof pw).toBe('string');
    expect((pw as string).length).toBeGreaterThan(0);
  });

  it('keyboard: S skips, I ignores, and the walk ends with a summary', async () => {
    render(<SecurityCheckup onClose={() => {}} />);
    const body = await screen.findByTestId('checkup-body');
    fireEvent.keyDown(body, { key: 's' }); // skip breached
    await waitFor(() => expect(screen.getByTestId('checkup-progress').textContent).toBe('2 of 6'));
    fireEvent.keyDown(body, { key: 'i' }); // ignore reused r1
    await waitFor(() => expect(api.ignoreFinding).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'r1', kind: 'reused' }));
    fireEvent.keyDown(body, { key: 's' });
    fireEvent.keyDown(body, { key: 's' });
    fireEvent.keyDown(body, { key: 's' });
    fireEvent.keyDown(body, { key: 's' });
    await screen.findByTestId('checkup-end');
    expect(screen.getByTestId('checkup-summary').textContent).toContain('Fixed 0');
    expect(screen.getByTestId('checkup-summary').textContent).toContain('Ignored 1');
    expect(screen.getByTestId('checkup-summary').textContent).toContain('Skipped 5');
  });

  it('Change on site opens the change-password page and advances', async () => {
    render(<SecurityCheckup onClose={() => {}} />);
    await screen.findByTestId('checkup-step');
    fireEvent.click(screen.getByTestId('checkup-change-on-site'));
    await waitFor(() => expect(api.openChangePassword).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'b1' }));
    await waitFor(() => expect(screen.getByTestId('checkup-progress').textContent).toBe('2 of 6'));
  });

  it('shows the score delta after an update recomputes the score', async () => {
    let score = 72;
    vi.mocked(api.watchtower).mockImplementation(async () => report({ score }) as never);
    render(<SecurityCheckup onClose={() => {}} />);
    await screen.findByTestId('checkup-step');
    score = 76;
    fireEvent.click(screen.getByTestId('checkup-ignore'));
    await screen.findByTestId('checkup-score-delta');
    expect(screen.getByTestId('checkup-score-delta').textContent).toContain('72 → 76');
  });

  it('missing 2FA findings offer Open item instead of password actions', async () => {
    vi.mocked(api.watchtower).mockResolvedValue(report({ weak: [], reused: [], old: [], breached: [] }) as never);
    render(<SecurityCheckup onClose={() => {}} />);
    await screen.findByTestId('checkup-step');
    expect(screen.queryByTestId('checkup-generate-update')).toBeNull();
    fireEvent.click(screen.getByTestId('checkup-open-item'));
    expect(useApp.getState().selectedItemId).toBe('m1');
  });
});
