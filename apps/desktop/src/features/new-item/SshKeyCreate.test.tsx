import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { CreateForm } from './CreateForm';
import { CATEGORIES } from './categories';
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const vault = { id: 'v1', name: 'Work', description: null, itemCount: 0, shared: false, members: [] };
const created = { item: { id: 'i1', vaultId: 'v1', category: 'ssh_key' as const, title: 'Laptop', subtitle: '', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '', createdAt: '' }, publicKey: 'ssh-ed25519 AAAAC3Nz test@x', fingerprint: 'SHA256:abc', keyType: 'ssh-ed25519' };
describe('SSH key create', () => {
  it('generates via api.sshGenerate and shows public key + fingerprint', async () => {
    vi.spyOn(api, 'listVaults').mockResolvedValue([vault]); const gen = vi.spyOn(api, 'sshGenerate').mockResolvedValue(created); const u = userEvent.setup();
    render(<CreateForm category="ssh_key" onBack={() => {}} onDone={() => {}} />);
    await u.type(screen.getByLabelText('Title'), 'Laptop'); await u.type(screen.getByLabelText('Comment'), 'me@laptop');
    await screen.findByRole('option', { name: 'Work' });
    await u.click(screen.getByRole('button', { name: 'Generate' }));
    expect(gen).toHaveBeenCalledWith({ vaultId: 'v1', title: 'Laptop', comment: 'me@laptop' });
    expect((await screen.findByTestId('ssh-public-key')).textContent).toBe('ssh-ed25519 AAAAC3Nz test@x');
    expect(screen.getByTestId('ssh-fingerprint').textContent).toBe('SHA256:abc');
  });
  it('imports with passphrase and surfaces backend errors', async () => {
    vi.spyOn(api, 'listVaults').mockResolvedValue([vault]); const imp = vi.spyOn(api, 'sshImport').mockRejectedValue(new Error('wrong passphrase')); const u = userEvent.setup();
    render(<CreateForm category="ssh_key" onBack={() => {}} onDone={() => {}} />);
    await screen.findByRole('option', { name: 'Work' });
    await u.click(screen.getByRole('radio', { name: 'Import OpenSSH key' }));
    await u.type(screen.getByLabelText('Private key'), 'PEM'); await u.type(screen.getByLabelText('Passphrase'), 'pw');
    await u.click(screen.getByRole('button', { name: 'Import' }));
    expect(imp).toHaveBeenCalledWith({ vaultId: 'v1', title: 'SSH Key', privateKey: 'PEM', passphrase: 'pw' });
    expect((await screen.findByRole('alert')).textContent).toMatch(/wrong passphrase/);
  });
  it('passkeys are not manually creatable', () => { expect(CATEGORIES.some(c => (c.id as string) === 'passkey')).toBe(false); });
});
