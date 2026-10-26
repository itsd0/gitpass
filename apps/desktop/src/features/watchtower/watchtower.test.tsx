import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import { api } from '../../api/client';
import { useSettings } from '../settings/useSettings';
const it3 = { id: 'i3', vaultId: 'v-private', category: 'login' as const, title: 'Fake Forum', subtitle: 'demo', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '', createdAt: '' };
let hibpOn = false;
beforeEach(() => {
  hibpOn = false; useSettings.setState({ settings: null });
  vi.spyOn(api, 'getSettings').mockImplementation(async () => ({ autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: hibpOn }));
  vi.spyOn(api, 'setSettings').mockImplementation(async (a: any) => { hibpOn = !!a.settings.hibpCheck; });
  vi.spyOn(api, 'listItems').mockResolvedValue([it3]);
  vi.spyOn(api, 'watchtower').mockImplementation(async (a: any) => { const on = !!a.hibp && hibpOn; return { score: on ? 62 : 71, weak: ['i3'], reused: [], old: [], breached: on ? ['i3'] : [], missingTotp: ['i3', 'i3x'], checkedAt: '', hibpChecked: on }; });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { gaugeDash, issueIds, scoreBand } from './score';
import { ScoreGauge, WatchtowerPage } from './WatchtowerPage';
import { useApp } from '../../lib/store';
describe('watchtower score', () => {
  it('bands and clamps', () => {
    expect(scoreBand(92).label).toBe('Excellent'); expect(scoreBand(70).label).toBe('Good');
    expect(scoreBand(50).label).toBe('Fair'); expect(scoreBand(10).label).toBe('Poor');
    expect(scoreBand(150).score).toBe(100); expect(scoreBand(-3).score).toBe(0);
    const d = gaugeDash(50); expect(d.filled).toBeCloseTo(d.arc / 2);
  });
  it('flattens reused groups uniquely', () => {
    expect(issueIds({ score: 0, weak: [], reused: [['a', 'b'], ['b', 'c']], old: [], breached: [], missingTotp: [], checkedAt: '', hibpChecked: false }, 'reused')).toEqual(['a', 'b', 'c']);
  });
  it('renders gauge', () => {
    render(<ScoreGauge score={71} />);
    expect(screen.getByRole('img', { name: 'Security score 71 of 100, Good' })).toBeTruthy();
  });
  it('page renders counts, HIBP opt-in, and selects item on click', async () => {
    render(<WatchtowerPage />);
    expect((await screen.findByTestId('score')).textContent).toBe('71');
    expect(screen.getByTestId('count-breached').textContent).toBe('—');
    expect(screen.getByTestId('count-missingTotp').textContent).toBe('2');
    fireEvent.click(screen.getByRole('switch', { name: 'Check for breached passwords' }));
    await waitFor(() => expect(screen.getByTestId('score').textContent).toBe('62'));
    expect(screen.getByTestId('count-breached').textContent).toBe('1');
    expect(hibpOn).toBe(true); // opt-in persisted to settings (default off)
    fireEvent.click(screen.getAllByText('Fake Forum')[0]);
    expect(useApp.getState().selectedItemId).toBe('i3');
    expect(useApp.getState().route).toEqual({ name: 'items', filter: { kind: 'vault', vaultId: 'v-private' } });
  });
});
