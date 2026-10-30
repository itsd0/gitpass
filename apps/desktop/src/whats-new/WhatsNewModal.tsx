import * as React from 'react';
import * as D from '@radix-ui/react-dialog';
import { ExternalLink, Sparkles, X } from 'lucide-react';
import { cn } from '../lib/utils';
import { Button } from '../ui';
import { useI18n, useT } from '../i18n';
import { useApp } from '../lib/store';
import { getReleaseHighlights } from './releases';
import { getLocalizedText, type WhatsNewShowMe } from './types';

export function WhatsNewModal() {
  const { modal, closeModal, setRoute } = useApp();
  const lang = useI18n((s) => s.lang);
  const t = useT();

  if (modal?.id !== 'whats-new') return null;

  const reqVersion = typeof modal.props?.version === 'string' ? modal.props.version : undefined;
  const release = getReleaseHighlights(reqVersion);
  const displayVersion = reqVersion || release?.version || 'latest';
  const cleanVersion = displayVersion.replace(/^v/i, '');
  const releaseUrl = `https://github.com/itsd0/gitpass/releases/tag/v${cleanVersion}`;

  const handleShowMe = (showMe: WhatsNewShowMe) => {
    closeModal();
    if (showMe.action) {
      showMe.action();
    } else if (showMe.route) {
      setRoute(showMe.route);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      const target = e.target as HTMLElement | null;
      // Allow standard Enter on buttons and links
      if (target?.tagName === 'BUTTON' || target?.tagName === 'A') return;
      e.preventDefault();
      closeModal();
    }
  };

  return (
    <D.Root open onOpenChange={(open) => !open && closeModal()}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-black/50 backdrop-blur-[2px] transition-opacity motion-reduce:transition-none" />
        <D.Content
          onKeyDown={handleKeyDown}
          aria-label={t('whatsNew.title', { version: displayVersion })}
          className={cn(
            'fixed left-1/2 top-1/2 z-50 w-[540px] max-w-[92vw] max-h-[85vh] -translate-x-1/2 -translate-y-1/2',
            'rounded-xl border border-border bg-panel p-6 shadow-2xl overflow-y-auto',
            'focus:outline-none transition-all motion-reduce:transition-none'
          )}
        >
          {/* Header icon */}
          <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-accent/15 text-accent">
            <Sparkles className="h-6 w-6 text-accent" aria-hidden />
          </div>

          <D.Title className="text-center text-lg font-semibold text-fg">
            {t('whatsNew.title', { version: displayVersion })}
          </D.Title>
          <D.Description className="sr-only">
            {t('about.whatsNewDesc')}
          </D.Description>

          <div className="mt-1.5 text-center">
            <a
              href={releaseUrl}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-xs text-accent hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            >
              {t('whatsNew.releaseNotes')}
              <ExternalLink className="h-3 w-3" aria-hidden />
            </a>
          </div>

          <D.Close
            aria-label="Close"
            className="absolute right-3.5 top-3.5 rounded p-1 text-muted hover:bg-hover hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          >
            <X size={16} />
          </D.Close>

          {/* Highlights */}
          <div className="mt-5 space-y-3" data-testid="whats-new-highlights">
            {release && release.highlights.length > 0 ? (
              release.highlights.map((h) => {
                const Icon = h.icon;
                return (
                  <div
                    key={h.id}
                    className="flex items-start gap-3.5 rounded-lg border border-border/70 bg-card/60 p-3.5"
                    data-testid={`whats-new-highlight-${h.id}`}
                  >
                    {Icon && (
                      <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-accent/10 text-accent">
                        <Icon size={18} aria-hidden />
                      </div>
                    )}
                    <div className="min-w-0 flex-1">
                      <div className="text-sm font-medium text-fg">
                        {getLocalizedText(h.title, lang)}
                      </div>
                      <div className="mt-0.5 text-xs text-muted leading-relaxed">
                        {getLocalizedText(h.description, lang)}
                      </div>
                    </div>
                    {h.showMe && (
                      <Button
                        variant="outline"
                        size="sm"
                        className="shrink-0 text-xs"
                        onClick={() => handleShowMe(h.showMe!)}
                        data-testid={`whats-new-show-me-${h.id}`}
                      >
                        {h.showMe.label ? getLocalizedText(h.showMe.label, lang) : t('whatsNew.showMe')}
                      </Button>
                    )}
                  </div>
                );
              })
            ) : (
              <p className="text-center text-sm text-muted py-4">
                {t('whatsNew.noHighlights')}
              </p>
            )}
          </div>

          {/* Footer with Got it button */}
          <div className="mt-6 flex justify-end">
            <Button
              variant="default"
              onClick={closeModal}
              className="min-w-[80px]"
              autoFocus
              data-testid="whats-new-got-it"
            >
              {t('whatsNew.gotIt')}
            </Button>
          </div>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}
