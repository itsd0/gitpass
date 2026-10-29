import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { SshKeyDetail, sshKeyBadgeLabel, GITHUB_SSH_NEW_URL } from './SshKeyDetail';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import { useI18n } from '../../i18n';
import type { ItemDetail } from '../../api/types';

const PUBLIC = 'ssh-ed25519 AAAAC3FAKEPUBLICKEY user@example.invalid';
const FINGERPRINT = 'SHA256:FAKEFINGERPRINT';
const PRIVATE = '-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEPRIVATEKEYDATA\n-----END OPENSSH PRIVATE KEY-----';

function detail(overrides: Partial<Record<string, string>> = {}): ItemDetail {
  const fields = [
    { id: 'private_key', label: 'private key', kind: 'concealed' as const, value: PRIVATE },
    { id: 'public_key', label: 'public key', kind: 'multiline' as const, value: PUBLIC },
    { id: 'fingerprint', label: 'fingerprint', kind: 'text' as const, value: FINGERPRINT },
    { id: 'key_type', label: 'key type', kind: 'text' as const, value: 'ssh-ed25519' },
  ];
  for (const [id, value] of Object.entries(overrides)) {
    const f = fields.find((x) => x.id === id);
    if (f) f.value = value;
    else fields.push({ id, label: id, kind: 'text' as const, value });
  }
  return {
    id: 'item-1', vaultId: 'v1', category: 'ssh_key', title: 'GitHub key', subtitle: '',
    favorite: false, archived: false, deletedAt: null, tags: [],
    updatedAt: '2026-10-29T00:00:00Z', createdAt: '2026-10-20T00:00:00Z',
    sections: [{ id: 'main', title: null, fields }],
    notes: null, attachments: [], history: [], hasTotp: false,
  };
}

const ctx = { vaultId: 'v1', itemId: 'item-1' };

describe('sshKeyBadgeLabel', () => {
  it('maps each key type to its display badge', () => {
    expect(sshKeyBadgeLabel('ssh-ed25519', null)).toBe('Ed25519');
    expect(sshKeyBadgeLabel('ecdsa-sha2-nistp256', null)).toBe('ECDSA P-256');
    expect(sshKeyBadgeLabel('ecdsa-sha2-nistp384', null)).toBe('ECDSA P-384');
    expect(sshKeyBadgeLabel('ssh-rsa', 3072)).toBe('RSA 3072');
    expect(sshKeyBadgeLabel('ssh-rsa', 2048)).toBe('RSA 2048');
  });
});

describe('SshKeyDetail (#249)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.setState({ lang: 'en' });
    useApp.setState({ route: { name: 'items', filter: { kind: 'all', sort: 'updated' } } });
    vi.spyOn(api, 'sshAgentInfo').mockResolvedValue({ socketPath: '/x/ssh-agent.sock', configLine: 'Host *', running: true, approveEachUse: false });
  });
  afterEach(() => { vi.restoreAllMocks(); });

  const wrap = (d: ItemDetail) => render(<TooltipProvider><SshKeyDetail detail={d} ctx={ctx} /></TooltipProvider>);

  it('shows the Ed25519 badge, fingerprint and public key, and never renders the private key', () => {
    const { container } = wrap(detail());
    expect(screen.getByTestId('ssh-key-badge')).toHaveTextContent('Ed25519');
    expect(screen.getByTestId('ssh-fingerprint')).toHaveTextContent(FINGERPRINT);
    expect(screen.getByTestId('ssh-public-key')).toHaveTextContent(PUBLIC);
    expect(container.textContent).not.toContain('FAKEPRIVATEKEYDATA');
    expect(screen.queryByTestId('ssh-weak-hint')).toBeNull();
  });

  it('shows RSA bit length in the badge and a Watchtower hint for RSA under 3072 bits', () => {
    wrap(detail({ key_type: 'ssh-rsa', bit_length: '2048', public_key: 'ssh-rsa AAAAC3FAKER SA user@example.invalid' }));
    expect(screen.getByTestId('ssh-key-badge')).toHaveTextContent('RSA 2048');
    expect(screen.getByTestId('ssh-weak-hint')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Watchtower' }));
    expect(useApp.getState().route).toEqual({ name: 'watchtower' });
  });

  it('Copy public key goes through api.copyField (Rust), not the DOM', async () => {
    const spy = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    wrap(detail());
    await act(async () => { fireEvent.click(screen.getByTestId('ssh-copy-public-key')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'item-1', fieldId: 'public_key' });
  });

  it('Add to GitHub copies the public key first, then opens the GitHub SSH key page', async () => {
    const spy = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    const openSpy = vi.fn();
    window.open = openSpy as unknown as typeof window.open;
    wrap(detail());
    await act(async () => { fireEvent.click(screen.getByTestId('ssh-add-to-github')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'item-1', fieldId: 'public_key' });
    expect(openSpy).toHaveBeenCalledWith(GITHUB_SSH_NEW_URL, '_blank', 'noopener,noreferrer');
    expect(GITHUB_SSH_NEW_URL).toBe('https://github.com/settings/ssh/new');
  });

  it('expands the public key on click and shows the agent status with a Settings link', async () => {
    wrap(detail());
    const row = screen.getByTestId('ssh-public-key');
    expect(row).toHaveClass('truncate');
    fireEvent.click(row);
    expect(row).not.toHaveClass('truncate');
    await screen.findByTestId('ssh-agent-status');
    expect(screen.getByTestId('ssh-agent-status')).toHaveTextContent('Running');
    fireEvent.click(screen.getByRole('button', { name: 'SSH Agent' }));
    expect(useApp.getState().route).toEqual({ name: 'settings', tab: 'ssh' });
  });

  it('renders nothing extra for a key with no public facts, and derives the type from the public key', () => {
    const d = detail();
    d.sections = [{ id: 'main', title: null, fields: [
      { id: 'public_key', label: 'public key', kind: 'multiline', value: 'ecdsa-sha2-nistp256 AAAAC3FAKE user@example.invalid' },
    ] }];
    wrap(d);
    expect(screen.getByTestId('ssh-key-badge')).toHaveTextContent('ECDSA P-256');
  });
});
