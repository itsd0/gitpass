import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import QRCode from 'qrcode';
import { api } from '../../../api/client';
import { AddDeviceDialog } from './AddDeviceDialog';
import { DevicesTab } from '../tabs/DevicesTab';

const QR = 'gitpass-pair:2:qr:FAKE';
const TOKEN = 'github_pat_FAKE_desktop_ui';

function prepared(expiresAt = Math.floor(Date.now() / 1000) + 120) {
  return { qr: QR, expiresAt, port: 9, addresses: ['192.0.2.10'] };
}

function open() {
  return render(<AddDeviceDialog open onOpenChange={() => {}} />);
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
});

describe('Add device', () => {
  it('shows a QR, then accepts the code the phone displayed', async () => {
    const user = userEvent.setup();
    const listen = vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockResolvedValue(undefined);
    const confirm = vi.spyOn(api, 'pairingHostConfirm').mockResolvedValue({ phase: 'done', error: null });
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    open();
    expect(await screen.findByAltText('Pairing QR code')).toBeTruthy();
    expect(QRCode.toDataURL).toHaveBeenCalledWith(QR, expect.anything());
    expect(screen.queryByText(/camera/i)).toBeNull();
    await user.type(await screen.findByLabelText('Code from the phone'), '123456');
    await user.click(screen.getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText(/Type the Master Password on the phone/)).toBeTruthy();
    expect(listen).toHaveBeenCalled();
    expect(confirm).toHaveBeenCalledWith({ code: '123456' });
    expect(document.body.textContent).not.toContain('github_pat_');
    expect(document.body.textContent).not.toContain('correct horse');
  });

  it('keeps the dialog open when the code is wrong', async () => {
    const user = userEvent.setup();
    vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockResolvedValue(undefined);
    vi.spyOn(api, 'pairingHostConfirm').mockResolvedValue({
      phase: 'code',
      error: 'pairing verification code does not match',
    });
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    open();
    await user.type(await screen.findByLabelText('Code from the phone'), '000000');
    await user.click(screen.getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText('That code does not match. Check the phone and try again.')).toBeTruthy();
    expect(screen.getByLabelText('Code from the phone')).toBeTruthy();
  });

  it('shows expiry without accepting a phone', async () => {
    const accept = vi.spyOn(api, 'pairingHostAccept').mockResolvedValue(undefined);
    vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared(1));
    const cancel = vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    open();
    expect(await screen.findByText('This pairing session expired. Start again.')).toBeTruthy();
    expect(accept).not.toHaveBeenCalled();
    expect(cancel).toHaveBeenCalled();
  });

  it('cancels the listener', async () => {
    const user = userEvent.setup();
    vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockImplementation(() => new Promise(() => {}));
    const cancel = vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    const onOpenChange = vi.fn();
    render(<AddDeviceDialog open onOpenChange={onOpenChange} />);
    await screen.findByAltText('Pairing QR code');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(cancel).toHaveBeenCalled();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('hides raw network errors', async () => {
    vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockRejectedValue(new Error('connection refused 203.0.113.8:9'));
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    open();
    expect(await screen.findByText('Pairing failed. Start again.')).toBeTruthy();
    expect(document.body.textContent).not.toContain('203.0.113.8');
    expect(screen.queryByText(/camera/i)).toBeNull();
  });

  it('explains a timeout as a network problem', async () => {
    vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockRejectedValue(new Error('pairing timed out'));
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    open();
    expect(
      await screen.findByText('The phone could not connect. Use the same Wi-Fi, and not a guest network that isolates devices.'),
    ).toBeTruthy();
  });

  it('uses a typed token only until the QR is shown', async () => {
    const user = userEvent.setup();
    const listen = vi
      .spyOn(api, 'pairingHostListen')
      .mockRejectedValueOnce(new Error('sync token is not configured'))
      .mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockResolvedValue(undefined);
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    open();
    await user.type(await screen.findByLabelText('Fine-grained personal access token'), TOKEN);
    await user.click(screen.getByRole('button', { name: 'Show QR code' }));
    await screen.findByAltText('Pairing QR code');
    expect(listen).toHaveBeenLastCalledWith({ token: TOKEN });
    expect(document.body.textContent).not.toContain(TOKEN);
    expect(screen.queryByLabelText('Fine-grained personal access token')).toBeNull();
  });

  it('offers Add device when the device list is empty', async () => {
    vi.spyOn(api, 'listDevices').mockResolvedValue([]);
    vi.spyOn(api, 'pairingHostListen').mockResolvedValue(prepared());
    vi.spyOn(api, 'pairingHostAccept').mockResolvedValue(undefined);
    vi.spyOn(api, 'pairingHostCancel').mockResolvedValue(undefined);
    const user = userEvent.setup();
    render(<DevicesTab />);
    await user.click(await screen.findByRole('button', { name: 'Add device' }));
    expect(await screen.findByRole('dialog', { name: 'Add a device' })).toBeTruthy();
    expect(screen.queryByText(/camera/i)).toBeNull();
  });
});
