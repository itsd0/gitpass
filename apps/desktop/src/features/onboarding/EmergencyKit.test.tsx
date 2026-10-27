import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react';
import { api } from '../../api/client';
import type { EmergencyKitData } from '../../api/types';
import { EmergencyKitFlow, KitSheet, formatKitDate, kitQrPayload } from './EmergencyKit';
import { dictionaries } from '../../i18n';
import { en } from '../../i18n/en';

const base: EmergencyKitData = { accountId: 'acc-1', vaultPath: '/tmp/v', remote: 'git@example.invalid:me/v.git', deviceName: 'Test Mac', createdAt: '2026-10-26T03:00:00Z', secretKeyAvailable: true, secretKey: null };
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('emergency kit', () => {
  it('QR payload only includes the Secret Key when asked and present', () => {
    expect(JSON.parse(kitQrPayload(base, true))).toEqual({ app: 'gitpass', v: '1', account: 'acc-1', vault: '/tmp/v', remote: 'git@example.invalid:me/v.git' });
    expect(JSON.parse(kitQrPayload({ ...base, secretKey: 'A3-X' }, true)).secretKey).toBe('A3-X');
    expect(JSON.parse(kitQrPayload({ ...base, secretKey: 'A3-X' }, false)).secretKey).toBeUndefined();
    expect(formatKitDate('2026-10-26T03:00:00Z')).toBe('2026-10-26'); expect(formatKitDate('nope')).toBe('nope');
  });
  it('renders the sheet with account, remote, device and an empty Secret Key box with actions', () => {
    const onRotate = vi.fn();
    render(<KitSheet data={base} onRotate={onRotate} onReveal={() => {}} />);
    expect(screen.getByText('acc-1')).toBeTruthy(); expect(screen.getByText('git@example.invalid:me/v.git')).toBeTruthy(); expect(screen.getByText('Test Mac')).toBeTruthy();
    expect(screen.getByTestId('kit-date').textContent).toBe('2026-10-26');
    expect(screen.getByTestId('kit-secret-key').textContent).toContain(en['kit.secretMissing']);
    fireEvent.click(screen.getByText(en['kit.rotateToPrint'])); expect(onRotate).toHaveBeenCalled();
  });
  it('reveals the Secret Key only after the master password is re-entered', async () => {
    const spy = vi.spyOn(api, 'emergencyKitData').mockImplementation(async (a: { password?: string } = {}) => {
      if (a.password === undefined) return base; if (a.password !== 'right') throw new Error('wrong master password'); return { ...base, secretKey: 'A3-SECRET' }; });
    render(<EmergencyKitFlow onClose={() => {}} />);
    await screen.findByTestId('emergency-kit-sheet');
    expect(screen.queryByText('A3-SECRET')).toBeNull();
    fireEvent.click(screen.getAllByText(en['kit.reveal'])[0]!);
    fireEvent.change(screen.getByLabelText(en['kit.masterPassword']), { target: { value: 'bad' } });
    fireEvent.submit(screen.getByLabelText(en['kit.masterPassword']).closest('form')!);
    await screen.findByText(/wrong master password/);
    fireEvent.click(screen.getAllByText(en['kit.reveal'])[0]!);
    fireEvent.change(screen.getByLabelText(en['kit.masterPassword']), { target: { value: 'right' } });
    fireEvent.submit(screen.getByLabelText(en['kit.masterPassword']).closest('form')!);
    await screen.findByText('A3-SECRET');
    expect(spy).toHaveBeenLastCalledWith({ password: 'right' });
  });
  it('rotate action rotates then reloads the kit with the new key', async () => {
    let rotated = false;
    vi.spyOn(api, 'emergencyKitData').mockImplementation(async () => (rotated ? { ...base, secretKey: 'A3-NEW' } : base));
    const rot = vi.spyOn(api, 'rotateSecretKey').mockImplementation(async () => { rotated = true; return { emergencyKit: 'x' }; });
    render(<EmergencyKitFlow onClose={() => {}} />);
    await screen.findByTestId('emergency-kit-sheet');
    fireEvent.click(screen.getByText(en['kit.rotateToPrint']));
    fireEvent.click(screen.getByText(en['kit.rotateConfirm']));
    await screen.findByText('A3-NEW'); expect(rot).toHaveBeenCalledTimes(1);
  });
  it('print button calls window.print and onPrinted', async () => {
    vi.spyOn(api, 'emergencyKitData').mockResolvedValue({ ...base, secretKey: 'A3-K' });
    const print = vi.fn(); vi.stubGlobal('print', print); window.print = print; const done = vi.fn();
    render(<EmergencyKitFlow onClose={() => {}} onPrinted={done} />);
    await screen.findByText('A3-K');
    fireEvent.click(screen.getByText(en['kit.print']));
    await waitFor(() => expect(done).toHaveBeenCalled()); expect(print).toHaveBeenCalled();
  });
  it('every kit.* key exists in English, Burmese, and Thai', () => {
    const keys = Object.keys(en).filter(k => k.startsWith('kit.'));
    expect(keys.length).toBeGreaterThan(25);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) {
        expect(d[k as keyof typeof en]).toBeTruthy();
      }
    }
  });
});
