import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { useToasts } from '../../ui/toast';
import { CreateForm } from '../new-item/CreateForm';
import { ItemEditor } from './ItemEditor';
import type { ItemDetail, VaultInfo } from '../../api/types';

describe('TOTP QR code scanning', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useToasts.setState({ list: [] });
    vi.spyOn(api, 'listVaults').mockResolvedValue([{ id: 'v1', name: 'Personal' } as VaultInfo]);
  });

  const getToastTitles = () => useToasts.getState().list.map((t) => t.title);

  it('CreateForm: renders Scan QR code button next to TOTP field and opens menu', async () => {
    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const scanBtn = await screen.findByTestId('totp-scan-button');
    expect(scanBtn).toBeTruthy();
    expect(scanBtn.textContent).toContain('Scan QR code');

    // Click button to open dropdown menu
    await userEvent.click(scanBtn);
    expect(await screen.findByTestId('scan-qr-screen')).toBeTruthy();
    expect(screen.getByTestId('scan-qr-clipboard')).toBeTruthy();
  });

  it('CreateForm: scans from screen, fills TOTP field, and offers title and username autofill', async () => {
    vi.spyOn(api, 'scanTotp').mockResolvedValueOnce({
      uri: 'otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub&algorithm=SHA1&digits=6&period=30',
      secret: 'JBSWY3DPEHPK3PXP',
      issuer: 'GitHub',
      account: 'octocat',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    });

    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);

    const screenOpt = await screen.findByTestId('scan-qr-screen');
    await userEvent.click(screenOpt);

    const totpInput = screen.getByLabelText('one-time password') as HTMLInputElement;
    await waitFor(() => {
      expect(totpInput.value).toBe(
        'otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub&algorithm=SHA1&digits=6&period=30'
      );
    });

    // Autofill offer banner is shown
    const suggestion = await screen.findByTestId('totp-autofill-suggestion');
    expect(suggestion.textContent).toContain('Fill title as “GitHub” and username as “octocat”?');

    // Apply suggestion
    const applyBtn = screen.getByTestId('totp-autofill-apply');
    await userEvent.click(applyBtn);

    const titleInput = screen.getByLabelText('Title') as HTMLInputElement;
    const userInput = screen.getByLabelText('username') as HTMLInputElement;
    expect(titleInput.value).toBe('GitHub');
    expect(userInput.value).toBe('octocat');

    // Suggestion banner is dismissed after apply
    expect(screen.queryByTestId('totp-autofill-suggestion')).toBeNull();
  });

  it('CreateForm: dismisses autofill offer without changing title or username', async () => {
    vi.spyOn(api, 'scanTotp').mockResolvedValueOnce({
      uri: 'otpauth://totp/Google:alice?secret=JBSWY3DPEHPK3PXP&issuer=Google',
      secret: 'JBSWY3DPEHPK3PXP',
      issuer: 'Google',
      account: 'alice',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    });

    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);
    await userEvent.click(await screen.findByTestId('scan-qr-clipboard'));

    const dismissBtn = await screen.findByTestId('totp-autofill-dismiss');
    await userEvent.click(dismissBtn);

    const titleInput = screen.getByLabelText('Title') as HTMLInputElement;
    const userInput = screen.getByLabelText('username') as HTMLInputElement;
    expect(titleInput.value).toBe('');
    expect(userInput.value).toBe('');
    expect(screen.queryByTestId('totp-autofill-suggestion')).toBeNull();
  });

  it('CreateForm: does not offer title autofill if title is already filled', async () => {
    vi.spyOn(api, 'scanTotp').mockResolvedValueOnce({
      uri: 'otpauth://totp/MyAccount?secret=JBSWY3DPEHPK3PXP&issuer=Service',
      secret: 'JBSWY3DPEHPK3PXP',
      issuer: 'Service',
      account: 'myuser',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    });

    render(
      <CreateForm
        category="login"
        initial={{ title: 'Existing Title' }}
        onBack={() => {}}
        onDone={() => {}}
      />
    );
    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);
    await userEvent.click(await screen.findByTestId('scan-qr-screen'));

    // Only offers username
    const suggestion = await screen.findByTestId('totp-autofill-suggestion');
    expect(suggestion.textContent).toContain('Fill username as “myuser”?');

    await userEvent.click(screen.getByTestId('totp-autofill-apply'));
    const titleInput = screen.getByLabelText('Title') as HTMLInputElement;
    const userInput = screen.getByLabelText('username') as HTMLInputElement;
    expect(titleInput.value).toBe('Existing Title');
    expect(userInput.value).toBe('myuser');
  });

  it('shows error toast when no QR code is found', async () => {
    vi.spyOn(api, 'scanTotp').mockRejectedValueOnce(new Error('No QR code found'));

    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);
    await userEvent.click(await screen.findByTestId('scan-qr-screen'));

    await waitFor(() => {
      expect(getToastTitles()).toContain('No QR code found');
    });
  });

  it('shows error toast when QR code is not a one-time code', async () => {
    vi.spyOn(api, 'scanTotp').mockRejectedValueOnce(new Error("This QR code isn't a one-time code"));

    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);
    await userEvent.click(await screen.findByTestId('scan-qr-clipboard'));

    await waitFor(() => {
      expect(getToastTitles()).toContain("This QR code isn't a one-time code");
    });
  });

  it('shows error toast when screen capture is cancelled', async () => {
    vi.spyOn(api, 'scanTotp').mockRejectedValueOnce(new Error('Screen capture was cancelled'));

    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);
    await userEvent.click(await screen.findByTestId('scan-qr-screen'));

    await waitFor(() => {
      expect(getToastTitles()).toContain('Screen capture was cancelled');
    });
  });

  it('CreateForm: dropping an image onto the TOTP field scans and fills it', async () => {
    vi.spyOn(api, 'scanTotp').mockResolvedValueOnce({
      uri: 'otpauth://totp/Dropbox:alex?secret=JBSWY3DPEHPK3PXP&issuer=Dropbox',
      secret: 'JBSWY3DPEHPK3PXP',
      issuer: 'Dropbox',
      account: 'alex',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    });

    render(<CreateForm category="login" onBack={() => {}} onDone={() => {}} />);
    const totpInput = await screen.findByLabelText('one-time password');
    const dropContainer = totpInput.parentElement!;

    const file = new File(['fake-png-bytes'], 'qr.png', { type: 'image/png' });
    fireEvent.dragOver(dropContainer, { dataTransfer: { types: ['Files'] } });
    fireEvent.drop(dropContainer, { dataTransfer: { files: [file] } });

    await waitFor(() => {
      expect(api.scanTotp).toHaveBeenCalledWith(expect.objectContaining({ source: 'image' }));
      expect((totpInput as HTMLInputElement).value).toContain('otpauth://totp/Dropbox:alex');
    });
  });

  it('ItemEditor: scans QR code, fills field, and offers title and username autofill', async () => {
    vi.spyOn(api, 'scanTotp').mockResolvedValueOnce({
      uri: 'otpauth://totp/Amazon:bob?secret=JBSWY3DPEHPK3PXP&issuer=Amazon',
      secret: 'JBSWY3DPEHPK3PXP',
      issuer: 'Amazon',
      account: 'bob',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    });

    const mockDetail: ItemDetail = {
      id: 'i1',
      vaultId: 'v1',
      category: 'login',
      title: 'Untitled',
      username: null,
      url: null,
      notes: null,
      tags: [],
      favorite: false,
      archived: false,
      hasPassword: false,
      hasTotp: false,
      sections: [
        {
          id: 'main',
          title: null,
          fields: [
            { id: 'username', label: 'username', kind: 'text', value: '' },
            { id: 'otp', label: 'one-time password', kind: 'totp', value: '' },
          ],
        },
      ],
      version: 1,
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    };

    render(<ItemEditor detail={mockDetail} vaults={[{ id: 'v1', name: 'Personal' } as VaultInfo]} />);

    const scanBtn = await screen.findByTestId('totp-scan-button');
    await userEvent.click(scanBtn);
    await userEvent.click(await screen.findByTestId('scan-qr-clipboard'));

    const suggestion = await screen.findByTestId('totp-autofill-suggestion');
    expect(suggestion.textContent).toContain('Fill title as “Amazon” and username as “bob”?');

    await userEvent.click(screen.getByTestId('totp-autofill-apply'));

    const titleInput = screen.getByLabelText('Title') as HTMLInputElement;
    const userInput = screen.getByLabelText('username value') as HTMLInputElement;
    expect(titleInput.value).toBe('Amazon');
    expect(userInput.value).toBe('bob');
  });
});
