import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { GitSigningDialog } from './GitSigningDialog';
import { ItemDetailPane } from './ItemDetail';
import { TooltipProvider } from '../../ui';
import { useApp } from '../../lib/store';
import { useItemsUI } from './state';
import { useI18n } from '../../i18n';
import { api } from '../../api/client';
import type { GitSigningConfig, ItemDetail, ItemSummary } from '../../api/types';

const mockSshKeySummary: ItemSummary = {
  id: 'ssh-item-1',
  vaultId: 'v_personal',
  category: 'ssh_key',
  title: 'Deploy Key',
  subtitle: 'ED25519 SHA256:FAKE123',
  favorite: false,
  archived: false,
  tags: ['dev'],
  updatedAt: 1700000000,
};

const mockLoginSummary: ItemSummary = {
  id: 'login-item-1',
  vaultId: 'v_personal',
  category: 'login',
  title: 'GitHub',
  subtitle: 'alice',
  favorite: false,
  archived: false,
  tags: [],
  updatedAt: 1700000000,
};

const mockSshKeyDetail: ItemDetail = {
  id: 'ssh-item-1',
  vaultId: 'v_personal',
  title: 'Deploy Key',
  subtitle: 'ED25519 SHA256:FAKE123',
  category: 'ssh_key',
  favorite: false,
  archived: false,
  sections: [
    {
      id: 'main',
      title: null,
      fields: [
        { id: 'public_key', label: 'public key', kind: 'multiline', value: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKE deploy@example.invalid', masked: null, diff: null },
        { id: 'fingerprint', label: 'fingerprint', kind: 'text', value: 'SHA256:FAKE123', masked: null, diff: null },
        { id: 'key_type', label: 'key type', kind: 'text', value: 'ssh-ed25519', masked: null, diff: null },
      ],
    },
  ],
  tags: ['dev'],
  attachments: [],
  history: [],
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T00:00:00Z',
};

const mockLoginDetail: ItemDetail = {
  id: 'login-item-1',
  vaultId: 'v_personal',
  title: 'GitHub',
  subtitle: 'alice',
  category: 'login',
  favorite: false,
  archived: false,
  sections: [],
  tags: [],
  attachments: [],
  history: [],
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T00:00:00Z',
};

const mockConfig: GitSigningConfig = {
  keyType: 'ssh-ed25519',
  publicKey: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKE deploy@example.invalid',
  signingKey: 'key::ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKE deploy@example.invalid',
  allowedSignersFile: '~/.ssh/allowed_signers',
  allowedSignersLine: 'deploy@example.invalid ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKE deploy@example.invalid',
  configLines: 'gpg.format=ssh\nuser.signingkey="key::ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKE deploy@example.invalid"\ncommit.gpgsign=true\ntag.gpgsign=true\ngpg.ssh.allowedSignersFile=~/.ssh/allowed_signers',
  email: 'deploy@example.invalid',
};

describe('GitSigningDialog', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.getState().setLang('en');
    useApp.setState({ locked: false, selectedItemId: null });
    useItemsUI.setState({ items: [mockSshKeySummary, mockLoginSummary] });
  });

  it('renders config lines and allowed signers when opened', async () => {
    vi.spyOn(api, 'sshGitSigningConfig').mockResolvedValue(mockConfig);

    render(
      <GitSigningDialog
        open={true}
        onOpenChange={() => {}}
        detail={mockSshKeyDetail}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('git-signing-code')).toHaveTextContent('gpg.format=ssh');
    });

    expect(screen.getByTestId('git-signing-code')).toHaveTextContent('user.signingkey="key::ssh-ed25519');
    expect(screen.getByTestId('allowed-signers-line')).toHaveTextContent('deploy@example.invalid ssh-ed25519');
  });

  it('copies configuration to clipboard on Copy button click', async () => {
    vi.spyOn(api, 'sshGitSigningConfig').mockResolvedValue(mockConfig);
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: writeTextSpy },
      configurable: true,
      writable: true,
    });

    render(
      <GitSigningDialog
        open={true}
        onOpenChange={() => {}}
        detail={mockSshKeyDetail}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('copy-git-signing-btn')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('copy-git-signing-btn'));
    expect(writeTextSpy).toHaveBeenCalledWith(mockConfig.configLines);
  });

  it('asks for confirmation before applying to ~/.gitconfig', async () => {
    vi.spyOn(api, 'sshGitSigningConfig').mockResolvedValue(mockConfig);
    const applySpy = vi.spyOn(api, 'sshApplyGitSigning').mockResolvedValue(mockConfig);

    const user = userEvent.setup();
    render(
      <GitSigningDialog
        open={true}
        onOpenChange={() => {}}
        detail={mockSshKeyDetail}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('apply-gitconfig-btn')).toBeInTheDocument();
    });

    // Clicking "Apply to ~/.gitconfig" reveals confirmation
    await user.click(screen.getByTestId('apply-gitconfig-btn'));
    expect(screen.getByTestId('apply-confirm-box')).toBeInTheDocument();
    expect(applySpy).not.toHaveBeenCalled();

    // Clicking confirm button triggers apply
    await user.click(screen.getByTestId('confirm-apply-gitconfig-btn'));
    expect(applySpy).toHaveBeenCalledWith({
      vaultId: 'v_personal',
      itemId: 'ssh-item-1',
      email: 'deploy@example.invalid',
      allowedSignersFile: '~/.ssh/allowed_signers',
      global: true,
    });

    await waitFor(() => {
      expect(screen.getByTestId('applied-success-box')).toBeInTheDocument();
    });
  });

  it('displays clear error on unsupported key type', async () => {
    vi.spyOn(api, 'sshGitSigningConfig').mockRejectedValue(
      new Error('Git commit signing supports Ed25519 and RSA keys; found ecdsa-sha2-nistp256')
    );

    render(
      <GitSigningDialog
        open={true}
        onOpenChange={() => {}}
        detail={mockSshKeyDetail}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('git-signing-error')).toBeInTheDocument();
    });

    expect(screen.getByTestId('git-signing-error')).toHaveTextContent('supports Ed25519 and RSA');
  });

  it('shows Use for Git signing button in ItemDetail for ssh_key items and not for login items', async () => {
    vi.spyOn(api, 'getItem').mockImplementation(async ({ itemId }) => {
      if (itemId === 'ssh-item-1') return mockSshKeyDetail;
      return mockLoginDetail;
    });
    vi.spyOn(api, 'listVaults').mockResolvedValue([
      { id: 'v_personal', name: 'Personal', description: null, color: '#f97316', icon: null, itemCount: 1, shared: false, members: [] },
    ]);
    vi.spyOn(api, 'listTags').mockResolvedValue([]);

    // 1. With SSH item selected
    useApp.setState({ selectedItemId: 'ssh-item-1' });
    const { unmount } = render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('use-for-git-signing-btn')).toBeInTheDocument();
    });

    unmount();

    // 2. With login item selected
    useApp.setState({ selectedItemId: 'login-item-1' });
    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.queryByTestId('use-for-git-signing-btn')).toBeNull();
    });
  });

  it('renders in Burmese correctly', async () => {
    vi.spyOn(api, 'sshGitSigningConfig').mockResolvedValue(mockConfig);
    useI18n.getState().setLang('my');

    render(
      <GitSigningDialog
        open={true}
        onOpenChange={() => {}}
        detail={mockSshKeyDetail}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('SSH ဖြင့် Git Commit လက်မှတ်ထိုးပါ')).toBeInTheDocument();
    });
  });

  it('renders in Thai correctly', async () => {
    vi.spyOn(api, 'sshGitSigningConfig').mockResolvedValue(mockConfig);
    useI18n.getState().setLang('th');

    render(
      <GitSigningDialog
        open={true}
        onOpenChange={() => {}}
        detail={mockSshKeyDetail}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('ลงชื่อ Git Commits ด้วย SSH')).toBeInTheDocument();
    });
  });
});
