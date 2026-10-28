import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import { api } from '../../api/client';
import { mock } from '../../api/mock';
import { useSettings } from '../settings/useSettings';
const it3 = { id: 'i3', vaultId: 'v-private', category: 'login' as const, title: 'Fake Forum', subtitle: 'demo', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '', createdAt: '', url: 'https://forum.example.org' };
const it3x = { ...it3, id: 'i3x', title: 'Fake 2FA Site', url: 'https://2fa.example.org' };
const itExp = { ...it3, id: 'i-exp', category: 'credit_card' as const, title: 'Expired Card', url: null };
const itSoon = { ...it3, id: 'i-soon', category: 'api_credential' as const, title: 'Expiring API Key', url: 'https://api.example.org' };
const itInsecure = { ...it3, id: 'i-insecure', title: 'Insecure Website', url: 'http://insecure.example.com/login?token=abc' };
let hibpOn = false;
beforeEach(() => {
  hibpOn = false; useSettings.setState({ settings: null });
  vi.spyOn(api, 'getSettings').mockImplementation(async () => ({ autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: hibpOn }));
  vi.spyOn(api, 'setSettings').mockImplementation(async (a: any) => { hibpOn = !!a.settings.hibpCheck; });
  vi.spyOn(api, 'listItems').mockResolvedValue([it3, it3x, itExp, itSoon, itInsecure]);
  vi.spyOn(api, 'watchtower').mockImplementation(async (a: any) => { const on = !!a.hibp && hibpOn; return { score: on ? 62 : 71, weak: ['i3'], reused: [], old: [], breached: on ? ['i3'] : [], missingTotp: ['i3', 'i3x'], totpAvailable: ['i3x'], passkeyAvailable: ['i3x'], expired: ['i-exp'], expiringSoon: ['i-soon'], insecureUrls: ['i-insecure'], checkedAt: '', hibpChecked: on }; });
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
    expect(issueIds({ score: 0, weak: [], reused: [['a', 'b'], ['b', 'c']], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [], checkedAt: '', hibpChecked: false }, 'reused')).toEqual(['a', 'b', 'c']);
  });
  it('shows site-specific 2FA instead of generic missing TOTP', () => {
    const r = { score: 0, weak: [], reused: [], old: [], breached: [], missingTotp: ['a', 'b'], totpAvailable: ['b'], passkeyAvailable: ['c'], checkedAt: '', hibpChecked: false };
    expect(issueIds(r, 'missingTotp')).toEqual(['a']);
    expect(issueIds(r, 'totpAvailable')).toEqual(['b']);
    expect(issueIds(r, 'passkeyAvailable')).toEqual(['c']);
  });
  it('renders gauge', () => {
    render(<ScoreGauge score={71} />);
    expect(screen.getByRole('img', { name: 'Security score 71 of 100, Good' })).toBeTruthy();
  });
  it('page renders counts, HIBP opt-in, and selects item on click', async () => {
    render(<WatchtowerPage />);
    expect((await screen.findByTestId('score')).textContent).toBe('71');
    expect(screen.getByTestId('count-breached').textContent).toBe('—');
    expect(screen.getByTestId('count-missingTotp').textContent).toBe('1'); // i3x is shown as "2FA available"
    expect(screen.getByTestId('count-totpAvailable').textContent).toBe('1');
    expect(screen.getByTestId('count-passkeyAvailable').textContent).toBe('1');
    fireEvent.click(screen.getByRole('switch', { name: 'Check for breached passwords' }));
    await waitFor(() => expect(screen.getByTestId('score').textContent).toBe('62'));
    expect(screen.getByTestId('count-breached').textContent).toBe('1');
    expect(hibpOn).toBe(true); // opt-in persisted to settings (default off)
    fireEvent.click(screen.getAllByText('Fake Forum')[0]);
    expect(useApp.getState().selectedItemId).toBe('i3');
    expect(useApp.getState().route).toEqual({ name: 'items', filter: { kind: 'vault', vaultId: 'v-private' } });
  });
  it('links 2FA/passkey items from their cards', async () => {
    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    expect(screen.getAllByText('Fake 2FA Site').length).toBe(2); // 2FA available + Passkeys available
    fireEvent.click(screen.getAllByText('Fake 2FA Site')[0]);
    expect(useApp.getState().selectedItemId).toBe('i3x');
    expect(useApp.getState().route).toEqual({ name: 'items', filter: { kind: 'vault', vaultId: 'v-private' } });
  });
  it('extracts expired and expiringSoon issue IDs', () => {
    const r = { score: 0, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [], expired: ['i-exp'], expiringSoon: ['i-soon'], checkedAt: '', hibpChecked: false };
    expect(issueIds(r, 'expired')).toEqual(['i-exp']);
    expect(issueIds(r, 'expiringSoon')).toEqual(['i-soon']);
  });
  it('renders expired and expiring soon cards with counts and links items', async () => {
    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    expect(screen.getByTestId('count-expired').textContent).toBe('1');
    expect(screen.getByTestId('count-expiringSoon').textContent).toBe('1');
    expect(screen.getByText('Expired Card')).toBeTruthy();
    expect(screen.getByText('Expiring API Key')).toBeTruthy();
    fireEvent.click(screen.getByText('Expired Card'));
    expect(useApp.getState().selectedItemId).toBe('i-exp');
  });
  it('extracts insecureUrls issue IDs and renders Unsecured websites card with actions', async () => {
    const r = { score: 0, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: ['i-insecure'], checkedAt: '', hibpChecked: false };
    expect(issueIds(r, 'insecureUrls')).toEqual(['i-insecure']);

    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    expect(screen.getByTestId('count-insecureUrls').textContent).toBe('1');
    expect(screen.getByText('Insecure Website')).toBeTruthy();
    expect(screen.getByText('http://insecure.example.com/login?token=abc')).toBeTruthy();

    const openHttpsLink = screen.getByRole('link', { name: 'Open HTTPS version for Insecure Website' }) as HTMLAnchorElement;
    expect(openHttpsLink).toBeTruthy();
    expect(openHttpsLink.href).toBe('https://insecure.example.com/login?token=abc');
    expect(openHttpsLink.target).toBe('_blank');

    const editBtn = screen.getByRole('button', { name: 'Edit URL for Insecure Website' });
    expect(editBtn).toBeTruthy();
    fireEvent.click(editBtn);
    expect(useApp.getState().selectedItemId).toBe('i-insecure');
    expect(useApp.getState().editing).toBe(true);
    expect(useApp.getState().route).toEqual({ name: 'items', filter: { kind: 'vault', vaultId: 'v-private' } });
  });
  it('renders unsecured websites card and per-item finding with mock data', async () => {
    vi.restoreAllMocks();
    useSettings.setState({ settings: null });
    vi.spyOn(api, 'getSettings').mockImplementation(mock.get_settings);
    vi.spyOn(api, 'listItems').mockImplementation(mock.list_items);
    vi.spyOn(api, 'watchtower').mockImplementation(mock.watchtower);

    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    expect(Number(screen.getByTestId('count-insecureUrls').textContent)).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Placeholder Forum').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('http://forum.example.io')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Open HTTPS version for Placeholder Forum' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Edit URL for Placeholder Forum' })).toBeTruthy();
  });

  it("ignores warning via dropdown menu with undo toast and updates score", async () => {
    let ignored: any[] = [];
    const ignoreSpy = vi.spyOn(api, "ignoreFinding").mockImplementation(async (a) => {
      ignored.push({ itemId: a.itemId, vaultId: a.vaultId, title: "Fake Forum", kind: a.kind, description: "Weak password" });
    });
    const unignoreSpy = vi.spyOn(api, "unignoreFinding").mockImplementation(async (a) => {
      ignored = ignored.filter((x) => x.itemId !== a.itemId || x.kind !== a.kind);
    });

    vi.spyOn(api, "watchtower").mockImplementation(async () => ({
      score: ignored.length > 0 ? 95 : 71,
      weak: ignored.some((x) => x.itemId === "i3" && x.kind === "weak") ? [] : ["i3"],
      reused: [],
      old: [],
      breached: [],
      missingTotp: [],
      totpAvailable: [],
      passkeyAvailable: [],
      expired: [],
      expiringSoon: [],
      insecureUrls: [],
      ignored,
      checkedAt: "",
      hibpChecked: false,
    }));

    render(<WatchtowerPage />);
    await screen.findByTestId("score");
    expect(screen.getByTestId("score").textContent).toBe("71");

    // Open options menu for Fake Forum in weak passwords card
    const moreBtn = screen.getAllByRole("button", { name: "More options for Fake Forum" })[0];
    fireEvent.pointerDown(moreBtn, { button: 0, ctrlKey: false });
    fireEvent.click(moreBtn);

    const ignoreItem = await screen.findByRole("menuitem", { name: /Ignore this warning/i });
    expect(ignoreItem).toBeTruthy();
    fireEvent.click(ignoreItem);

    await waitFor(() => expect(ignoreSpy).toHaveBeenCalledWith({ vaultId: "v-private", itemId: "i3", kind: "weak" }));
    await waitFor(() => expect(screen.getByTestId("score").textContent).toBe("95"));

    // Check Ignored section
    expect(screen.getByTestId("count-ignored").textContent).toBe("1");
    fireEvent.click(screen.getByTestId("toggle-ignored-section"));

    expect(screen.getByText("Weak password")).toBeTruthy();
    const stopBtn = screen.getByRole("button", { name: "Stop ignoring for Fake Forum" });
    expect(stopBtn).toBeTruthy();

    fireEvent.click(stopBtn);
    await waitFor(() => expect(unignoreSpy).toHaveBeenCalledWith({ vaultId: "v-private", itemId: "i3", kind: "weak" }));
    await waitFor(() => expect(screen.getByTestId("score").textContent).toBe("71"));
  });

  it("keyboard navigation: Shift+F10 opens ignore menu on finding chip", async () => {
    render(<WatchtowerPage />);
    await screen.findByTestId("score");

    const chip = screen.getAllByTestId("finding-chip-i3")[0];
    fireEvent.keyDown(chip, { key: "F10", shiftKey: true });

    expect(await screen.findByRole("menuitem", { name: /Ignore this warning/i })).toBeTruthy();
  });
});
