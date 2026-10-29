import { useEffect, useState } from 'react';
import { Copy, ExternalLink } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail as Detail, SshAgentInfo } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import { cn } from '../../lib/utils';
import { Badge, Button, toast } from '../../ui';
import { errText } from './actions';

/** GitHub's "new SSH key" page. No API call is made; the public key is copied first. */
export const GITHUB_SSH_NEW_URL = 'https://github.com/settings/ssh/new';

/** Field ids of the SSH facts rendered by this section (see crates/gitpass-core/src/ssh.rs). */
export const SSH_DETAIL_FIELD_IDS = ['public_key', 'fingerprint', 'key_type', 'bit_length'] as const;

/** Display label for a key type, e.g. `ssh-ed25519` -> `Ed25519`, `ssh-rsa` + 3072 -> `RSA 3072`. */
export function sshKeyBadgeLabel(keyType: string | null | undefined, bitLength: number | null | undefined): string {
  switch (keyType) {
    case 'ssh-ed25519': return 'Ed25519';
    case 'ecdsa-sha2-nistp256': return 'ECDSA P-256';
    case 'ecdsa-sha2-nistp384': return 'ECDSA P-384';
    case 'ecdsa-sha2-nistp521': return 'ECDSA P-521';
    case 'ssh-rsa': return bitLength ? `RSA ${bitLength}` : 'RSA';
    case 'ssh-dss': return 'DSA';
    case null:
    case undefined:
    case '': return 'SSH';
    default: return keyType;
  }
}

function fieldValue(detail: Detail, id: string): string | null {
  for (const s of detail.sections) {
    const f = s.fields.find((x) => x.id === id);
    if (f?.value) return f.value;
  }
  return null;
}

function hasField(detail: Detail, id: string): boolean {
  return detail.sections.some((s) => s.fields.some((f) => f.id === id));
}

/**
 * First-class SSH Key detail section, rendered under the item title.
 * Only public facts are shown here: key type, fingerprint and public key.
 * The private key is deliberately never read or rendered by this component;
 * it stays in the generic (concealed) field row below, with its existing
 * reveal/copy behaviour.
 */
export function SshKeyDetail({ detail, ctx }: { detail: Detail; ctx: { vaultId: string; itemId: string } }) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const [agent, setAgent] = useState<SshAgentInfo | null>(null);

  const publicKey = fieldValue(detail, 'public_key');
  const fingerprint = fieldValue(detail, 'fingerprint');
  const rawKeyType = fieldValue(detail, 'key_type') ?? publicKey?.split(/\s+/)[0] ?? null;
  const bitLengthRaw = fieldValue(detail, 'bit_length');
  const bitLength = bitLengthRaw ? parseInt(bitLengthRaw, 10) : null;
  const bits = bitLength != null && Number.isFinite(bitLength) ? bitLength : null;
  const badge = sshKeyBadgeLabel(rawKeyType, bits);
  const weakRsa = rawKeyType === 'ssh-rsa' && bits != null && bits < 3072;

  useEffect(() => {
    let on = true;
    api.sshAgentInfo().then((info) => { if (on) setAgent(info); }).catch(() => {});
    return () => { on = false; };
  }, []);

  if (!publicKey && !fingerprint && !rawKeyType) return null;

  const copyText = async (fieldId: string, value: string | null, toastMsg: string) => {
    try {
      if (hasField(detail, fieldId)) await api.copyField({ ...ctx, fieldId });
      else if (value) await navigator.clipboard?.writeText(value);
      toast(toastMsg);
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  const copyPublicKey = () => copyText('public_key', publicKey, t('ssh.detail.copiedPublicKey'));

  const addToGithub = async () => {
    await copyPublicKey();
    toast(t('ssh.detail.addToGithubToast'));
    window.open(GITHUB_SSH_NEW_URL, '_blank', 'noopener,noreferrer');
  };

  return (
    <div className="mb-4 rounded-lg border border-border bg-card px-4 py-3" data-testid="ssh-key-detail">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone="accent" data-testid="ssh-key-badge">{badge}</Badge>
        {weakRsa && (
          <span className="text-xs text-warn" data-testid="ssh-weak-hint">
            {t('watchtower.weakSshKeyDesc')}{' '}
            <Button variant="link" size="sm" onClick={() => useApp.getState().setRoute({ name: 'watchtower' })}>
              {t('nav.watchtower')}
            </Button>
          </span>
        )}
      </div>

      {fingerprint && (
        <div className="mt-2.5 flex items-center gap-2">
          <span className="w-20 shrink-0 text-xs text-label">{t('ssh.detail.fingerprint')}</span>
          <span className="min-w-0 flex-1 truncate font-mono text-[13px]" data-testid="ssh-fingerprint">{fingerprint}</span>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={t('ssh.detail.copyFingerprint')}
            onClick={() => void copyText('fingerprint', fingerprint, t('common.copied'))}
          >
            <Copy size={13} />
          </Button>
        </div>
      )}

      {publicKey && (
        <div className="mt-2.5">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs text-label">{t('ssh.detail.publicKey')}</span>
            <Button
              variant="ghost"
              size="sm"
              aria-label={expanded ? t('ssh.detail.hidePublicKey') : t('ssh.detail.showPublicKey')}
              aria-expanded={expanded}
              onClick={() => setExpanded(!expanded)}
            >
              {expanded ? t('ssh.detail.hidePublicKey') : t('ssh.detail.showPublicKey')}
            </Button>
          </div>
          <button
            type="button"
            className={cn(
              'mt-1 block w-full text-left font-mono text-[13px] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
              !expanded && 'truncate',
              expanded && 'whitespace-pre-wrap break-all',
            )}
            aria-label={expanded ? t('ssh.detail.hidePublicKey') : t('ssh.detail.showPublicKey')}
            aria-expanded={expanded}
            onClick={() => setExpanded(!expanded)}
            data-testid="ssh-public-key"
          >
            {publicKey}
          </button>
          <div className="mt-2 flex flex-wrap items-center gap-2">
            <Button variant="default" size="sm" onClick={() => void copyPublicKey()} data-testid="ssh-copy-public-key">
              <Copy size={13} />{t('ssh.detail.copyPublicKey')}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void addToGithub()} data-testid="ssh-add-to-github">
              <ExternalLink size={13} />{t('ssh.detail.addToGithub')}
            </Button>
          </div>
        </div>
      )}

      {agent && (
        <div className="mt-2.5 flex flex-wrap items-center gap-2 text-xs text-muted" data-testid="ssh-agent-status">
          <Badge tone={agent.running ? 'ok' : 'default'}>
            {agent.running ? t('ssh.running') : t('ssh.stopped')}
          </Badge>
          <Button variant="link" size="sm" onClick={() => useApp.getState().setRoute({ name: 'settings', tab: 'ssh' })}>
            {t('settings.ssh')}
          </Button>
        </div>
      )}
    </div>
  );
}
