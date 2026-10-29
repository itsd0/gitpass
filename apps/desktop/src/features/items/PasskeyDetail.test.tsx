import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { PasskeyDetail, shortenCredentialId } from './PasskeyDetail';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import { useItemsUI } from './state';
import { useI18n } from '../../i18n';
import type { ItemDetail } from '../../api/types';

const CRED = 'QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo12345678';
const PRIVATE = 'FAKEPRIVATEKEYBASE64DATA';

function detail(overrides: Partial<ItemDetail> = {}, fieldOverrides: Record<string, string> = {}): ItemDetail {
  const fields = [
    { id: 'rp_id', label: 'website', kind: 'text' as const, value: 'github.example.com' },
    { id: 'user_name', label: 'username', kind: 'text' as const, value: 'demo@example.invalid' },
    { id: 'credential_id', label: 'credential id', kind: 'text' as const, value: CRED },
    { id: 'user_handle', label: 'user handle', kind: 'text' as const, value: 'QUJD' },
    { id: 'private_key', label: 'private key', kind: 'concealed' as const, value: PRIVATE },
    { id: 'sign_count', label: 'sign count', kind: 'text' as const, value: '3' },
  ];
  for (const [id, value] of Object.entries(fieldOverrides)) {
    const f = fields.find((x) => x.id === id);
    if (f) f.value = value;
    else fields.push({ id, label: id, kind: 'text' as const, value });
  }
  return {
    id: 'item-1', vaultId: 'v1', category: 'passkey', title: 'GitHub passkey', subtitle: '',
    favorite: false, archived: false, deletedAt: null, tags: [],
    updatedAt: '2026-10-29T00:00:00Z', createdAt: '2026-10-20T00:00:00Z',
    lastUsed: Math.floor(Date.now() / 1000) - 3600,
    sections: [{ id: 'main', title: null, fields }],
    notes: null, attachments: [], history: [], hasTotp: false,
    ...overrides,
  };
}

const ctx = { vaultId: 'v1', itemId: 'item-1' };

describe('shortenCredentialId', () => {
  it('shortens to first 8 … last 8 and leaves short ids whole', () => {
    expect(shortenCredentialId(CRED)).toBe(`${CRED.slice(0, 8)}…${CRED.slice(-8)}`);
    expect(shortenCredentialId('short-id')).toBe('short-id');
  });
});

describe('PasskeyDetail (#254)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.setState({ lang: 'en' });
    useApp.setState({ route: { name: 'items', filter: { kind: 'all', sort: 'updated' } }, selectedItemId: null });
    useItemsUI.setState({ items: [] });
  });
  afterEach(() => { vi.restoreAllMocks(); });

  const wrap = (d: ItemDetail) => render(<TooltipProvider><PasskeyDetail detail={d} ctx={ctx} /></TooltipProvider>);

  it('shows site, account, shortened credential id and sign count, and never renders the private key', () => {
    const { container } = wrap(detail());
    expect(screen.getByTestId('passkey-site')).toHaveTextContent('github.example.com');
    expect(screen.getByTestId('passkey-account')).toHaveTextContent('demo@example.invalid');
    expect(screen.getByTestId('passkey-credential-id')).toHaveTextContent(shortenCredentialId(CRED));
    expect(screen.getByTestId('passkey-credential-id').textContent).not.toContain(CRED);
    expect(screen.getByTestId('passkey-sign-count')).toHaveTextContent('3');
    expect(screen.getByTestId('passkey-created')).toBeInTheDocument();
    expect(screen.getByTestId('passkey-last-used')).toHaveTextContent('Today');
    expect(container.textContent).not.toContain('FAKEPRIVATEKEYBASE64DATA');
  });

  it('shows "Never used" when there is no last-used timestamp', () => {
    wrap(detail({ lastUsed: undefined }));
    expect(screen.getByTestId('passkey-last-used')).toHaveTextContent('Never used');
  });

  it('renders from dev-mock-style labels when ids are random', () => {
    const d = detail();
    d.sections = [{ id: 's1', title: null, fields: [
      { id: 'f-a', label: 'site', kind: 'text' as const, value: 'webauthn.example.invalid' },
      { id: 'f-b', label: 'user name', kind: 'text' as const, value: 'demo@example.invalid' },
      { id: 'f-c', label: 'credential id', kind: 'text' as const, value: 'FAKE-cred-id' },
      { id: 'f-d', label: 'private key', kind: 'concealed' as const, value: PRIVATE },
      { id: 'f-e', label: 'sign count', kind: 'text' as const, value: '3' },
    ] }];
    wrap(d);
    expect(screen.getByTestId('passkey-site')).toHaveTextContent('webauthn.example.invalid');
    expect(screen.getByTestId('passkey-account')).toHaveTextContent('demo@example.invalid');
    expect(screen.getByTestId('passkey-credential-id')).toHaveTextContent('FAKE-cred-id');
  });

  it('Copy account and Copy credential ID go through api.copyField (Rust), not the DOM', async () => {
    const spy = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    wrap(detail());
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Copy account' })); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'item-1', fieldId: 'user_name' });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Copy credential ID' })); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'item-1', fieldId: 'credential_id' });
  });

  it('Open site goes through api.openPasskeySite (Rust-derived URL), not window.open', async () => {
    const spy = vi.spyOn(api, 'openPasskeySite').mockResolvedValue(undefined);
    wrap(detail());
    await act(async () => { fireEvent.click(screen.getByTestId('passkey-open-site')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'item-1' });
  });

  it('links to a login item saved for the same site', () => {
    useItemsUI.setState({ items: [
      { id: 'login-1', vaultId: 'v1', category: 'login', title: 'GitHub login', subtitle: '', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '2026-10-29T00:00:00Z', createdAt: '2026-10-20T00:00:00Z', url: 'https://github.example.com/login' },
      { id: 'login-2', vaultId: 'v1', category: 'login', title: 'Other login', subtitle: '', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '2026-10-29T00:00:00Z', createdAt: '2026-10-20T00:00:00Z', url: 'https://other.example.com' },
    ] });
    wrap(detail());
    expect(screen.getByTestId('passkey-also-login')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'GitHub login' }));
    expect(useApp.getState().selectedItemId).toBe('login-1');
  });

  it('renders nothing when there are no passkey facts', () => {
    const d = detail();
    d.sections = [{ id: 'main', title: null, fields: [
      { id: 'private_key', label: 'private key', kind: 'concealed' as const, value: PRIVATE },
    ] }];
    const { container } = wrap(d);
    expect(container.firstChild).toBeNull();
  });
});
