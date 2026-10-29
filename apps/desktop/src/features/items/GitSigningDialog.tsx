import { useEffect, useState } from 'react';
import { AlertTriangle, Check, Copy, GitBranch, Loader2, Terminal } from 'lucide-react';
import { api } from '../../api/client';
import type { GitSigningConfig, ItemDetail } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import { Button, Dialog, DialogContent } from '../../ui';
import { toast } from '../../ui/toast';
import { errText } from './actions';

export interface GitSigningDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  detail: ItemDetail;
  triggerRef?: React.RefObject<HTMLElement | null>;
}

export function GitSigningDialog({ open, onOpenChange, detail, triggerRef }: GitSigningDialogProps) {
  const t = useT();
  const locked = useApp((s) => s.locked);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [config, setConfig] = useState<GitSigningConfig | null>(null);
  const [copied, setCopied] = useState(false);
  const [copiedAuthSock, setCopiedAuthSock] = useState(false);
  const [confirmingApply, setConfirmingApply] = useState(false);
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);

  // If vault locks or dialog closes, dismiss
  useEffect(() => {
    if (locked && open) {
      onOpenChange(false);
    }
  }, [locked, open, onOpenChange]);

  useEffect(() => {
    if (!open) {
      setLoading(false);
      setError(null);
      setConfig(null);
      setCopied(false);
      setCopiedAuthSock(false);
      setConfirmingApply(false);
      setApplying(false);
      setApplied(false);
    }
  }, [open, detail.id]);

  useEffect(() => {
    if (!open || locked) return;

    let cancelled = false;
    setLoading(true);
    setError(null);

    async function loadConfig() {
      try {
        const res = await api.sshGitSigningConfig({
          vaultId: detail.vaultId,
          itemId: detail.id,
        });
        if (cancelled) return;
        setConfig(res);
      } catch (e) {
        if (cancelled) return;
        setError(errText(e) || t('ssh.gitSigningUnsupportedKeyType'));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void loadConfig();
    return () => {
      cancelled = true;
    };
  }, [open, locked, detail.id, detail.vaultId, t]);

  const handleCopy = async () => {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(config.configLines);
      setCopied(true);
      toast(t('ssh.copiedConfig'));
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  };

  const handleCopyAuthSock = async () => {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(config.exportAuthSock);
      setCopiedAuthSock(true);
      toast(t('ssh.copiedAuthSock'));
      setTimeout(() => setCopiedAuthSock(false), 2000);
    } catch {
      // ignore
    }
  };

  const handleApply = async () => {
    if (!config) return;
    setApplying(true);
    try {
      await api.sshApplyGitSigning({
        vaultId: detail.vaultId,
        itemId: detail.id,
        email: config.email,
        allowedSignersFile: config.allowedSignersFile,
        global: true,
      });
      setApplied(true);
      setConfirmingApply(false);
      toast(t('ssh.gitSigningApplied'));
    } catch (e) {
      toast(errText(e), 'danger');
    } finally {
      setApplying(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('ssh.gitSigningTitle')}
        description={t('ssh.gitSigningSubtitle')}
        className="w-[520px] max-w-[95vw]"
        onCloseAutoFocus={(e) => {
          if (triggerRef?.current) {
            e.preventDefault();
            triggerRef.current.focus();
          }
        }}
      >
        <div className="space-y-4">
          {loading ? (
            <div className="flex h-40 items-center justify-center text-muted" data-testid="git-signing-loading">
              <Loader2 className="h-6 w-6 animate-spin" />
            </div>
          ) : error ? (
            <div
              className="rounded-lg border border-danger/40 bg-danger/10 p-4 text-sm text-danger"
              role="alert"
              data-testid="git-signing-error"
            >
              {error}
            </div>
          ) : config ? (
            <>
              {/* Shell environment / SSH agent socket */}
              <div>
                <div className="mb-1.5 flex items-center justify-between text-xs font-medium text-fg/80">
                  <span className="flex items-center gap-1.5">
                    <Terminal size={13} className="text-accent" />
                    {t('ssh.authSockTitle')}
                  </span>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={handleCopyAuthSock}
                    className="h-7 text-xs"
                    data-testid="copy-auth-sock-btn"
                  >
                    {copiedAuthSock ? <Check size={12} className="text-accent" /> : <Copy size={12} />}
                    {copiedAuthSock ? t('common.copied') : t('ssh.copyAuthSock')}
                  </Button>
                </div>
                <pre
                  className="overflow-x-auto rounded-lg border border-border bg-panel p-2.5 font-mono text-xs leading-relaxed text-fg select-all"
                  data-testid="auth-sock-code"
                >
                  {config.exportAuthSock}
                </pre>
                {!config.authSockActive && (
                  <div
                    className="mt-2 flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 p-2.5 text-xs text-amber-800 dark:text-amber-300"
                    data-testid="auth-sock-warning"
                  >
                    <AlertTriangle size={14} className="shrink-0 mt-0.5" />
                    <span>{t('ssh.authSockWarning')}</span>
                  </div>
                )}
              </div>

              {/* Git configuration lines */}
              <div>
                <div className="mb-1.5 flex items-center justify-between text-xs font-medium text-fg/80">
                  <span className="flex items-center gap-1.5">
                    <GitBranch size={13} className="text-accent" />
                    {t('ssh.gitSigningConfig')}
                  </span>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={handleCopy}
                    className="h-7 text-xs"
                    data-testid="copy-git-signing-btn"
                  >
                    {copied ? <Check size={12} className="text-accent" /> : <Copy size={12} />}
                    {copied ? t('common.copied') : t('ssh.copyConfig')}
                  </Button>
                </div>
                <pre
                  className="overflow-x-auto rounded-lg border border-border bg-panel p-3 font-mono text-xs leading-relaxed text-fg select-all"
                  data-testid="git-signing-code"
                >
                  {config.configLines}
                </pre>
              </div>

              {/* Allowed signers entry */}
              <div>
                <div className="mb-1 text-xs font-medium text-muted">
                  {t('ssh.allowedSigners')} ({config.allowedSignersFile})
                </div>
                <pre
                  className="overflow-x-auto rounded-md border border-border/80 bg-panel/70 p-2 font-mono text-[11px] text-muted-fg select-all break-all whitespace-pre-wrap"
                  data-testid="allowed-signers-line"
                >
                  {config.allowedSignersLine}
                </pre>
              </div>

              {/* Confirmation box if apply was clicked */}
              {confirmingApply ? (
                <div
                  className="rounded-lg border border-accent/40 bg-accent/5 p-3.5 space-y-2.5"
                  data-testid="apply-confirm-box"
                >
                  <div className="text-sm font-semibold text-fg">{t('ssh.applyConfirmTitle')}</div>
                  <div className="text-xs text-muted leading-relaxed">{t('ssh.applyConfirmDesc')}</div>
                  <div className="flex justify-end gap-2 pt-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => setConfirmingApply(false)}
                      disabled={applying}
                    >
                      {t('common.cancel')}
                    </Button>
                    <Button
                      type="button"
                      variant="default"
                      size="sm"
                      onClick={handleApply}
                      disabled={applying}
                      data-testid="confirm-apply-gitconfig-btn"
                    >
                      {applying ? <Loader2 size={13} className="animate-spin" /> : null}
                      {t('ssh.applyConfirmBtn')}
                    </Button>
                  </div>
                </div>
              ) : applied ? (
                <div
                  className="space-y-1.5 rounded-lg border border-green-500/30 bg-green-500/10 p-3 text-xs font-medium text-green-700 dark:text-green-400"
                  data-testid="applied-success-box"
                >
                  <div className="flex items-center gap-2">
                    <Check size={15} />
                    <span>{t('ssh.gitSigningApplied')}</span>
                  </div>
                  {!config.authSockActive && (
                    <div className="text-[11px] text-green-800/90 dark:text-green-300/90 pl-6 font-normal">
                      {t('ssh.appliedAuthSockHint')}
                    </div>
                  )}
                </div>
              ) : null}

              {/* Bottom Actions */}
              <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-border/60">
                {!confirmingApply && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => setConfirmingApply(true)}
                    disabled={applied}
                    data-testid="apply-gitconfig-btn"
                    className="text-xs"
                  >
                    {t('ssh.applyToGitconfig')}
                  </Button>
                )}
                <div className="ml-auto">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => onOpenChange(false)}
                    data-testid="git-signing-close-btn"
                  >
                    {t('common.close')}
                  </Button>
                </div>
              </div>
            </>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
