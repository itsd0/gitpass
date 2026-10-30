import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import fs from 'fs';
import path from 'path';
import { cleanup } from '@testing-library/react';
import { api } from '../../api/client';
import { mock } from '../../api/mock';
import { useSettings } from '../settings/useSettings';
import { useI18n, translate } from '../../i18n';
import { Toaster, useToasts } from '../../ui';
const it3 = { id: 'i3', vaultId: 'v-private', category: 'login' as const, title: 'Fake Forum', subtitle: 'demo', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '', createdAt: '', url: 'https://forum.example.org', changePasswordUrl: 'https://forum.example.org/.well-known/change-password' };
const it3x = { ...it3, id: 'i3x', title: 'Fake 2FA Site', url: 'https://2fa.example.org', changePasswordUrl: 'https://2fa.example.org/.well-known/change-password' };
const itExp = { ...it3, id: 'i-exp', category: 'credit_card' as const, title: 'Expired Card', url: null, changePasswordUrl: undefined };
const itSoon = { ...it3, id: 'i-soon', category: 'api_credential' as const, title: 'Expiring API Key', url: 'https://api.example.org', changePasswordUrl: 'https://api.example.org/.well-known/change-password' };
const itInsecure = { ...it3, id: 'i-insecure', title: 'Insecure Website', url: 'http://insecure.example.com/login?token=abc', changePasswordUrl: 'https://insecure.example.com/.well-known/change-password' };
const itDupOlder = { ...it3, id: 'dup-older', title: 'Site Duplicate 1', subtitle: 'alex', url: 'https://dup.example.com', updatedAt: '2026-10-10T00:00:00Z', changePasswordUrl: 'https://dup.example.com/.well-known/change-password' };
const itDupNewer = { ...it3, id: 'dup-newer', title: 'Site Duplicate 2', subtitle: 'alex', url: 'https://dup.example.com', updatedAt: '2026-10-20T00:00:00Z', changePasswordUrl: 'https://dup.example.com/.well-known/change-password' };
let hibpOn = false;
beforeEach(() => {
  hibpOn = false; useSettings.setState({ settings: null });
  vi.spyOn(api, 'getSettings').mockImplementation(async () => ({ autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: hibpOn }));
  vi.spyOn(api, 'setSettings').mockImplementation(async (a: any) => { hibpOn = !!a.settings.hibpCheck; });
  vi.spyOn(api, 'listItems').mockResolvedValue([it3, it3x, itExp, itSoon, itInsecure, itDupOlder, itDupNewer]);
  vi.spyOn(api, 'archiveItem').mockResolvedValue(undefined);
  vi.spyOn(api, 'watchtower').mockImplementation(async (a: any) => { const on = !!a.hibp && hibpOn; return { score: on ? 62 : 71, weak: ['i3'], reused: [], old: [], breached: on ? ['i3'] : [], missingTotp: ['i3', 'i3x'], totpAvailable: ['i3x'], passkeyAvailable: ['i3x'], expired: ['i-exp'], expiringSoon: ['i-soon'], insecureUrls: ['i-insecure'], duplicates: [['dup-older', 'dup-newer']], checkedAt: '', hibpChecked: on }; });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); useI18n.getState().setLang('en'); useToasts.setState({ list: [] }); });
import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { gaugeDash, issueIds, scoreBand } from './score';
import { ScoreGauge, WatchtowerPage } from './WatchtowerPage';
import { useApp } from '../../lib/store';
describe('watchtower score', () => {
  it('bands and clamps', () => {
    expect(scoreBand(92).labelKey).toBe('excellent'); expect(scoreBand(70).labelKey).toBe('good');
    expect(scoreBand(50).labelKey).toBe('fair'); expect(scoreBand(10).labelKey).toBe('poor');
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
  it('renders gauge localized in Burmese and Thai', () => {
    useI18n.getState().setLang('my');
    render(<ScoreGauge score={71} />);
    expect(screen.getByRole('img', { name: 'လုံခြုံရေးရမှတ် 100 တွင် 71၊ ကောင်း' })).toBeTruthy();
    expect(screen.getByText('ကောင်း')).toBeTruthy();
    cleanup();
    useI18n.getState().setLang('th');
    render(<ScoreGauge score={92} />);
    expect(screen.getByRole('img', { name: 'คะแนนความปลอดภัย 92 จาก 100, ยอดเยี่ยม' })).toBeTruthy();
    expect(screen.getByText('ยอดเยี่ยม')).toBeTruthy();
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
      duplicates: [],
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

  it('extracts duplicates issue IDs uniquely', () => {
    const r = { score: 0, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [['d1', 'd2'], ['d2', 'd3']], ignored: [], checkedAt: '', hibpChecked: false };
    expect(issueIds(r, 'duplicates')).toEqual(['d1', 'd2', 'd3']);
  });

  it('renders duplicate logins card and Archive older calls archiveItem for all but the newest item', async () => {
    const archiveSpy = vi.spyOn(api, 'archiveItem');
    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    expect(screen.getByTestId('count-duplicates').textContent).toBe('2');
    expect(screen.getByText('Group 1 · 2 items')).toBeTruthy();
    expect(screen.getByText('Site Duplicate 1')).toBeTruthy();
    expect(screen.getByText('Site Duplicate 2')).toBeTruthy();
    // Archive older button is present with group title in aria-label
    const archiveOlderBtn = screen.getByRole('button', { name: /Archive older/i });
    expect(archiveOlderBtn).toBeTruthy();
    expect(archiveOlderBtn.getAttribute('aria-label')).toBe('Archive older: Group 1 · 2 items');
    fireEvent.click(archiveOlderBtn);
    await waitFor(() => {
      expect(archiveSpy).toHaveBeenCalledWith({ vaultId: 'v-private', itemId: 'dup-older', archived: true });
    });
    // Newest item should NOT have been archived
    expect(archiveSpy).not.toHaveBeenCalledWith(expect.objectContaining({ itemId: 'dup-newer' }));

    // Open button opens item
    const openBtns = screen.getAllByRole('button', { name: 'Open' });
    fireEvent.click(openBtns[0]);
    // The items in the duplicate group are sorted newest first, so openBtns[0] corresponds to dup-newer
    expect(useApp.getState().selectedItemId).toBe('dup-newer');
  });

  it("offers Change password for password findings and unsecured rows", async () => {
    const openSpy = vi.spyOn(api, "openChangePassword").mockResolvedValue(undefined);
    render(<WatchtowerPage />);
    await screen.findByTestId("score");

    // Weak password chip: icon button labelled with the item title.
    fireEvent.click(screen.getAllByRole("button", { name: "Change password for Fake Forum" })[0]);
    expect(openSpy).toHaveBeenCalledWith({ vaultId: "v-private", itemId: "i3" });

    // Unsecured website row: same action next to Open HTTPS / Edit URL.
    fireEvent.click(screen.getByRole("button", { name: "Change password for Insecure Website" }));
    expect(openSpy).toHaveBeenCalledWith({ vaultId: "v-private", itemId: "i-insecure" });

    // Duplicate login rows offer it as well.
    fireEvent.click(screen.getByRole("button", { name: "Change password for Site Duplicate 2" }));
    expect(openSpy).toHaveBeenCalledWith({ vaultId: "v-private", itemId: "dup-newer" });
  });

  it("hides Change password when the login has no usable URL", async () => {
    const noUrl = { ...it3, id: "i-nourl", title: "No URL Login", url: null, changePasswordUrl: undefined };
    vi.spyOn(api, "listItems").mockResolvedValue([noUrl]);
    vi.spyOn(api, "watchtower").mockResolvedValue({
      score: 50, weak: ["i-nourl"], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [],
      passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], ignored: [], checkedAt: "", hibpChecked: false,
    });
    render(<WatchtowerPage />);
    await screen.findByTestId("score");

    expect(screen.getByText("No URL Login")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Change password for No URL Login" })).toBeNull();
  });

  it("localizes Change password aria-labels in Thai", async () => {
    vi.mocked(api.getSettings).mockResolvedValue({
      autoLockMinutes: 10, clipboardClearSecs: 30, theme: "dark", language: "th", biometricUnlock: false, quickAccessHotkey: "CmdOrCtrl+Shift+Space", hibpCheck: false,
    });
    const openSpy = vi.spyOn(api, "openChangePassword").mockResolvedValue(undefined);
    render(<WatchtowerPage />);
    await screen.findByTestId("score");
    fireEvent.click((await screen.findAllByRole("button", { name: "เปลี่ยนรหัสผ่านสำหรับ Fake Forum" }))[0]);
    expect(openSpy).toHaveBeenCalledWith({ vaultId: "v-private", itemId: "i3" });
  });
  it("displays localized weak reasons in the weak passwords card", async () => {
    vi.spyOn(api, "watchtower").mockResolvedValue({
      score: 70,
      weak: ["i3", "i-site"],
      weakReasons: {
        i3: "contains the username",
        "i-site": "contains the site name",
      },
      reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [],
      passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], ignored: [], checkedAt: "", hibpChecked: false,
    });
    const siteItem = { ...it3, id: "i-site", title: "GitHub Site Login" };
    vi.spyOn(api, "listItems").mockResolvedValue([it3, siteItem]);

    render(<WatchtowerPage />);
    await screen.findByTestId("score");

    expect(screen.getByTestId("chip-reason-i3").textContent).toBe("(contains the username)");
    expect(screen.getByTestId("chip-reason-i-site").textContent).toBe("(contains the site name)");
  });

  it("localizes weak reasons in Thai and Burmese", async () => {
    vi.mocked(api.getSettings).mockResolvedValue({
      autoLockMinutes: 10, clipboardClearSecs: 30, theme: "dark", language: "th", biometricUnlock: false, quickAccessHotkey: "CmdOrCtrl+Shift+Space", hibpCheck: false,
    });
    vi.spyOn(api, "watchtower").mockResolvedValue({
      score: 70,
      weak: ["i3"],
      weakReasons: { i3: "contains the username" },
      reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [],
      passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], ignored: [], checkedAt: "", hibpChecked: false,
    });
    render(<WatchtowerPage />);
    await screen.findByTestId("score");
    expect(screen.getByTestId("chip-reason-i3").textContent).toBe("(มีชื่อผู้ใช้อยู่ในรหัสผ่าน)");
  });
  it("extracts weakSshKey issue IDs and renders Weak SSH keys card", async () => {
    const r = { score: 0, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], weakSshKeys: ["i-weak-ssh"], checkedAt: "", hibpChecked: false };
    expect(issueIds(r, "weakSshKey")).toEqual(["i-weak-ssh"]);

    render(<WatchtowerPage />);
    await screen.findByTestId("score");
    expect(screen.getByTestId("count-weakSshKey")).toBeInTheDocument();
  });

  it("localizes weak SSH keys card in English, Burmese, and Thai", () => {
    useI18n.getState().setLang("en");
    expect(translate("en", "watchtower.weakSshKey")).toBe("Weak SSH keys");
    useI18n.getState().setLang("my");
    expect(translate("my", "watchtower.weakSshKey")).toBe("အားနည်းသော SSH သော့များ");
    useI18n.getState().setLang("th");
    expect(translate("th", "watchtower.weakSshKey")).toBe("คีย์ SSH ที่ไม่ปลอดภัย");
  });

  it("renders unused items with last activity and archives one with Undo", async () => {
    vi.mocked(api.watchtower).mockResolvedValue({
      score: 80, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [],
      expired: [], expiringSoon: [], insecureUrls: [], duplicates: [],
      unused: [{ itemId: 'i3', days: 400, lastUsed: true }, { itemId: 'i3x', days: 800, lastUsed: false }],
      ignored: [], checkedAt: '', hibpChecked: false,
    });
    const archiveSpy = vi.spyOn(api, 'archiveItem').mockResolvedValue(undefined);

    render(<><Toaster /><WatchtowerPage /></>);
    await screen.findByTestId('score');
    expect(screen.getByTestId('count-unused').textContent).toBe('2');
    expect(screen.getByTestId('unused-item-i3')).toBeTruthy();
    expect(screen.getByText('Last used 13 months ago')).toBeTruthy();
    expect(screen.getByText('Last edited 27 months ago')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Archive Fake Forum' }));
    await waitFor(() => expect(archiveSpy).toHaveBeenCalledWith({ vaultId: 'v-private', itemId: 'i3', archived: true }));
    fireEvent.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(archiveSpy).toHaveBeenCalledWith({ vaultId: 'v-private', itemId: 'i3', archived: false }));
  });

  it("ignores an unused warning from the row menu", async () => {
    vi.mocked(api.watchtower).mockResolvedValue({
      score: 80, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [],
      expired: [], expiringSoon: [], insecureUrls: [], duplicates: [],
      unused: [{ itemId: 'i3x', days: 800, lastUsed: false }],
      ignored: [], checkedAt: '', hibpChecked: false,
    });
    const ignoreSpy = vi.spyOn(api, 'ignoreFinding').mockResolvedValue(undefined);

    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    const more = screen.getByRole('button', { name: 'More options for Fake 2FA Site' });
    fireEvent.keyDown(more, { key: 'Enter', code: 'Enter' });
    fireEvent.click(await screen.findByRole('menuitem', { name: /Ignore this warning/i }));
    await waitFor(() => expect(ignoreSpy).toHaveBeenCalledWith({ vaultId: 'v-private', itemId: 'i3x', kind: 'unused' }));
  });

  it("persists the unused threshold and shows the section as off at 0 months", async () => {
    vi.mocked(api.getSettings).mockResolvedValue({
      autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: 'en', biometricUnlock: false,
      quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false, watchtowerUnusedMonths: 0,
    });
    const saveSpy = vi.mocked(api.setSettings).mockImplementation(async (a: any) => ({ ...a.settings }));
    vi.mocked(api.watchtower).mockResolvedValue({
      score: 90, weak: [], reused: [], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [],
      expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], unused: [], ignored: [], checkedAt: '', hibpChecked: false,
    });

    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    const select = screen.getByTestId('unused-months-select') as HTMLSelectElement;
    expect(select.value).toBe('0');
    expect(screen.getByTestId('count-unused').textContent).toBe('—');
    expect(screen.getByText('Choose a threshold above to find logins you no longer use.')).toBeTruthy();

    fireEvent.change(select, { target: { value: '24' } });
    await waitFor(() => expect(saveSpy).toHaveBeenCalledWith({ settings: expect.objectContaining({ watchtowerUnusedMonths: 24 }) }));
    await waitFor(() => expect((screen.getByTestId('unused-months-select') as HTMLSelectElement).value).toBe('24'));
  });

  it("has no hard-coded Watchtower copy in the page source", () => {
    const source = fs.readFileSync(path.resolve(__dirname, 'WatchtowerPage.tsx'), 'utf8');
    const banned = [
      '>Re-check<', '>Watchtower<', 'Weak passwords', 'Reused passwords', 'Old passwords', 'Breached passwords',
      'Missing 2FA', 'Last checked ', 'breach check included', 'breach check off',
      'Enable the breach check above to scan.', '>No issues<', 'Off by default. When on, gitpass hashes',
    ];
    for (const copy of banned) expect(source.includes(copy), `WatchtowerPage.tsx should not contain: ${copy}`).toBe(false);
  });

  it("renders localized Watchtower copy in Burmese and Thai, including the language-aware date", async () => {
    const iso = '2026-10-28T12:00:00Z';
    vi.mocked(api.watchtower).mockResolvedValue({
      score: 71, weak: ['i3'], reused: [], old: [], breached: [], missingTotp: ['i3', 'i3x'], totpAvailable: ['i3x'],
      passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], checkedAt: iso, hibpChecked: false,
    });
    const english = [
      'Re-check',
      'Your security score reflects weak, reused, old and breached passwords and missing two-factor authentication.',
      'Weak passwords', 'Easy to guess. Generate a strong replacement.',
      'Reused passwords', 'Same password on multiple sites.',
      'Old passwords', 'Not changed in a long time.',
      'Breached passwords', 'Found in known data breaches (HIBP).',
      'Missing 2FA', 'Logins with a URL but no one-time password saved.',
      'Off by default. When on, gitpass hashes each password with SHA-1 locally and sends only the',
      'Enable the breach check above to scan.',
      'No issues',
    ];
    for (const lang of ['my', 'th'] as const) {
      useSettings.setState({ settings: { autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: lang, biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false } });
      useI18n.getState().setLang(lang);
      render(<WatchtowerPage />);
      await screen.findByTestId('score');
      const text = document.body.textContent ?? '';
      for (const copy of english) expect(text.includes(copy), `${lang} still shows English: ${copy}`).toBe(false);
      expect(screen.getByText(translate(lang, 'watchtower.weakPasswords'))).toBeTruthy();
      expect(screen.getByText(translate(lang, 'watchtower.recheck'))).toBeTruthy();
      expect(text).toContain(new Date(iso).toLocaleString(lang));
      cleanup();
      useSettings.setState({ settings: null });
    }
    useI18n.getState().setLang('en');
  });

  it("re-renders the card list when the language changes", async () => {
    useSettings.setState({ settings: { autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false } });
    render(<WatchtowerPage />);
    await screen.findByTestId('score');
    expect(screen.getByText('Weak passwords')).toBeTruthy();
    act(() => useI18n.getState().setLang('th'));
    expect(await screen.findByText('รหัสผ่านที่อ่อนแอ')).toBeTruthy();
    expect(screen.queryByText('Weak passwords')).toBeNull();
    useSettings.setState({ settings: null });
  });
});
