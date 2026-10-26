import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { ShareDialog } from './ShareDialog';
const vault = { id: 'v-private', name: 'Private', description: null, itemCount: 1, shared: false, members: [] };
const KEY = 'gitpass-x25519:' + btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => i + 1)));
const FPR = '0123456789abcdef0123456789abcdef';
beforeEach(() => { vi.spyOn(api, 'fingerprintOf').mockResolvedValue(FPR); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
describe('ShareDialog', () => {
  it('requires the out-of-band fingerprint confirmation before sharing', async () => {
    const spy = vi.spyOn(api, 'shareVault').mockResolvedValue({ fingerprint: 'AAAA', label: 'Alex', isSelf: false });
    const onShared = vi.fn(); const u = userEvent.setup();
    render(<ShareDialog vault={vault} open onOpenChange={() => {}} onShared={onShared} />);
    const submit = screen.getByTestId('share-submit');
    expect(submit).toBeDisabled();
    await u.type(screen.getByPlaceholderText('gitpass-x25519:…'), KEY);
    expect(await screen.findByTestId('recipient-fingerprint')).toHaveTextContent('0123');
    expect(api.fingerprintOf).toHaveBeenCalledWith({ publicKey: KEY });
    expect(submit).toBeDisabled(); // valid key but not confirmed
    await u.click(submit); expect(spy).not.toHaveBeenCalled();
    await u.click(await screen.findByRole('checkbox'));
    expect(submit).toBeEnabled();
    await u.click(submit);
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ vaultId: 'v-private', publicKey: KEY, label: null }));
    expect(onShared).toHaveBeenCalled();
  });
  it('resets confirmation when the key changes', async () => {
    const u = userEvent.setup(); render(<ShareDialog vault={vault} open onOpenChange={() => {}} />);
    await u.type(screen.getByPlaceholderText('gitpass-x25519:…'), KEY); await u.click(await screen.findByRole('checkbox'));
    await u.type(screen.getByPlaceholderText('gitpass-x25519:…'), 'x');
    expect(screen.getByTestId('share-submit')).toBeDisabled();
    await u.clear(screen.getByPlaceholderText('gitpass-x25519:…')); await u.type(screen.getByPlaceholderText('gitpass-x25519:…'), KEY);
    expect(await screen.findByRole('checkbox')).not.toBeChecked(); expect(screen.getByTestId('share-submit')).toBeDisabled();
  });
  it('shows the error when Rust rejects a well-shaped key', async () => {
    vi.spyOn(api, 'fingerprintOf').mockRejectedValue(new Error('invalid public key'));
    const u = userEvent.setup(); render(<ShareDialog vault={vault} open onOpenChange={() => {}} />);
    await u.type(screen.getByPlaceholderText('gitpass-x25519:…'), KEY);
    await waitFor(() => expect(screen.getByTestId('share-submit')).toBeDisabled());
    expect(screen.queryByTestId('recipient-fingerprint')).toBeNull();
  });
  it('rejects invalid keys', async () => {
    const u = userEvent.setup(); render(<ShareDialog vault={vault} open onOpenChange={() => {}} />);
    await u.type(screen.getByPlaceholderText('gitpass-x25519:…'), 'not-a-key');
    expect(screen.getByRole('alert')).toHaveTextContent('does not look like a public key'); expect(screen.queryByRole('checkbox')).toBeNull();
  });
});
