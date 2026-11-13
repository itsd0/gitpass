import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../../api/client';
import { AddDeviceDialog, pairingErrorKey } from './AddDeviceDialog';
import { DevicesTab } from '../tabs/DevicesTab';

const OFFER = 'gitpass-pair:1:offer:sid.pub.1700000120';
const ANNOUNCE = 'gitpass-pair:1:announce:sid.pub.host.1700000120';
const GRANT = 'gitpass-pair:1:grant:sid.pub.host.1700000120.ciphertext';

function pickImage() {
  const input = screen.getByTestId('pair-image') as HTMLInputElement;
  const file = new File([new Uint8Array([1, 2, 3])], 'qr.png', { type: 'image/png' });
  fireEvent.change(input, { target: { files: [file] } });
}

describe('Add device (QR pairing, #458)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    // jsdom has no camera: the dialog falls back to choosing an image.
    Object.defineProperty(navigator, 'mediaDevices', { value: undefined, configurable: true });
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('scans the offer, takes the phone code, shows the grant and never shows a desktop code', async () => {
    const scan = vi.spyOn(api, 'pairingScanOffer').mockResolvedValue(OFFER);
    const prepare = vi
      .spyOn(api, 'pairingHostPrepare')
      .mockResolvedValue({ announce: ANNOUNCE, expiresAt: Math.floor(Date.now() / 1000) + 120 });
    const release = vi.spyOn(api, 'pairingHostRelease').mockResolvedValue(GRANT);
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<AddDeviceDialog open onClose={onClose} />);
    expect(await screen.findByTestId('pair-camera-error')).toBeTruthy();
    fireEvent.change(screen.getByTestId('pair-token'), { target: { value: 'github_pat_FAKE_ui_test' } });
    pickImage();
    await screen.findByTestId('pair-announce-qr');
    expect(scan).toHaveBeenCalledWith({ dataBase64: expect.stringMatching(/^data:image\/png;base64,/) });
    expect(prepare).toHaveBeenCalledWith({ offer: OFFER, token: 'github_pat_FAKE_ui_test' });
    // The dialog shows no 6-digit number of its own.
    expect(document.body.textContent).not.toMatch(/\b\d{6}\b/);
    expect(screen.getByTestId('pair-remaining').textContent).toMatch(/Expires in 1[12]\d s/);
    const confirm = screen.getByTestId('pair-confirm') as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('pair-code'), { target: { value: '12a3456' } });
    expect((screen.getByTestId('pair-code') as HTMLInputElement).value).toBe('123456');
    fireEvent.click(confirm);
    await screen.findByTestId('pair-grant-qr');
    expect(release).toHaveBeenCalledWith({ code: '123456' });
    expect(screen.getByText(/Master Password on the phone/)).toBeTruthy();
    fireEvent.click(screen.getByTestId('pair-done'));
    expect(onClose).toHaveBeenCalledWith(true);
  });

  it('a wrong code keeps the session; too many wrong codes ends it', async () => {
    vi.spyOn(api, 'pairingScanOffer').mockResolvedValue(OFFER);
    vi.spyOn(api, 'pairingHostPrepare').mockResolvedValue({ announce: ANNOUNCE, expiresAt: Math.floor(Date.now() / 1000) + 120 });
    const release = vi
      .spyOn(api, 'pairingHostRelease')
      .mockRejectedValueOnce(new Error('pairing verification code does not match'))
      .mockRejectedValueOnce(new Error('too many wrong pairing codes'));
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    render(<AddDeviceDialog open onClose={() => {}} />);
    pickImage();
    await screen.findByTestId('pair-announce-qr');
    fireEvent.change(screen.getByTestId('pair-code'), { target: { value: '000000' } });
    fireEvent.click(screen.getByTestId('pair-confirm'));
    expect(await screen.findByText('That code does not match. Check the phone and try again.')).toBeTruthy();
    expect(screen.getByTestId('pair-announce-qr')).toBeTruthy();
    expect((screen.getByTestId('pair-code') as HTMLInputElement).value).toBe('');
    fireEvent.change(screen.getByTestId('pair-code'), { target: { value: '111111' } });
    fireEvent.click(screen.getByTestId('pair-confirm'));
    expect((await screen.findByTestId('pair-failure')).textContent).toBe('Too many wrong codes. Start again on the phone.');
    expect(release).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId('pair-grant-qr')).toBeNull();
  });

  it('expiry ends the session, hides the QR and cancels on the backend', async () => {
    vi.spyOn(api, 'pairingScanOffer').mockResolvedValue(OFFER);
    vi.spyOn(api, 'pairingHostPrepare').mockResolvedValue({ announce: ANNOUNCE, expiresAt: Date.now() / 1000 + 1 });
    const cancel = vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    render(<AddDeviceDialog open onClose={() => {}} />);
    pickImage();
    await screen.findByTestId('pair-announce-qr');
    expect((await screen.findByTestId('pair-failure', {}, { timeout: 4000 })).textContent).toBe('This pairing request expired. Start again on the phone.');
    expect(screen.queryByTestId('pair-announce-qr')).toBeNull();
    expect(cancel).toHaveBeenCalled();
  });

  it('refuses a QR code that is not a pairing offer without calling prepare', async () => {
    vi.spyOn(api, 'pairingScanOffer').mockRejectedValue(new Error("This QR code isn't a gitpass pairing code"));
    const prepare = vi.spyOn(api, 'pairingHostPrepare');
    render(<AddDeviceDialog open onClose={() => {}} />);
    pickImage();
    expect(await screen.findByText('That QR code is not a gitpass pairing code.')).toBeTruthy();
    expect(prepare).not.toHaveBeenCalled();
  });

  it('a missing or classic token is explained and the typed token is cleared on close', async () => {
    vi.spyOn(api, 'pairingScanOffer').mockResolvedValue(OFFER);
    vi.spyOn(api, 'pairingHostPrepare').mockRejectedValue(new Error('pairing payload field rejected'));
    const cancel = vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    const onClose = vi.fn();
    const { rerender } = render(<AddDeviceDialog open onClose={onClose} />);
    fireEvent.change(screen.getByTestId('pair-token'), { target: { value: 'ghp_FAKEclassic' } });
    pickImage();
    expect((await screen.findByTestId('pair-failure')).textContent).toBe('Use a fine-grained personal access token. It starts with github_pat_.');
    expect(document.body.textContent).not.toContain('ghp_FAKEclassic');
    fireEvent.click(screen.getByTestId('pair-restart'));
    fireEvent.click(screen.getByTestId('pair-cancel'));
    expect(onClose).toHaveBeenCalledWith(false);
    expect(cancel).toHaveBeenCalled();
    rerender(<AddDeviceDialog open={false} onClose={onClose} />);
    rerender(<AddDeviceDialog open onClose={onClose} />);
    await waitFor(() => expect((screen.getByTestId('pair-token') as HTMLInputElement).value).toBe(''));
  });

  it('maps backend errors to fixed UI text', () => {
    expect(pairingErrorKey('sync is not configured')).toBe('devices.add.notConfigured');
    expect(pairingErrorKey('sync token is not configured')).toBe('devices.add.noToken');
    expect(pairingErrorKey('pairing session already used')).toBe('devices.add.used');
    expect(pairingErrorKey('pairing session expired')).toBe('devices.add.expired');
    expect(pairingErrorKey('something else')).toBeNull();
  });

  it('the Devices tab offers Add device even with no devices', async () => {
    vi.spyOn(api, 'listDevices').mockResolvedValue([]);
    render(<DevicesTab />);
    fireEvent.click(await screen.findByTestId('add-device'));
    expect((await screen.findAllByText('Add a device')).length).toBeGreaterThan(0);
    expect(screen.getByTestId('pair-token')).toBeTruthy();
  });
});
