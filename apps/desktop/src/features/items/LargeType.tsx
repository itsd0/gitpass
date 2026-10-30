import { useEffect, useMemo } from 'react';
import * as D from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { Dialog } from '../../ui';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import type { FeatureDef } from '../registry';

import { charKind, kindClass, type CharKind } from '../../lib/secretText';
import { useColorPasswords } from '../settings/colorPasswords';

export { charKind };
export type { CharKind };

export interface CharCell {
  char: string;
  pos: number;
  kind: CharKind;
}

export function groupCharacters(value: string, groupSize = 8): CharCell[][] {
  if (!value) return [];
  const chars = Array.from(value);
  const groups: CharCell[][] = [];
  for (let i = 0; i < chars.length; i += groupSize) {
    const chunk = chars.slice(i, i + groupSize).map((char, offset) => ({
      char,
      pos: i + offset + 1,
      kind: charKind(char),
    }));
    groups.push(chunk);
  }
  return groups;
}

export function LargeTypeDialog({
  open,
  onOpenChange,
  title,
  value,
  triggerRef,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  value: string;
  triggerRef?: React.RefObject<HTMLElement | null>;
}) {
  const t = useT();
  const locked = useApp((s) => s.locked);

  useEffect(() => {
    if (!open) return;
    const onLocked = () => onOpenChange(false);
    window.addEventListener('gitpass:locked', onLocked);
    return () => window.removeEventListener('gitpass:locked', onLocked);
  }, [open, onOpenChange]);

  useEffect(() => {
    if (open && locked) {
      onOpenChange(false);
    }
  }, [open, locked, onOpenChange]);

  const colored = useColorPasswords((s) => s.enabled);
  const groups = useMemo(() => groupCharacters(value), [value]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm" />
        <D.Content
          className={cn(
            'fixed left-1/2 top-1/2 z-50 w-auto max-w-[94vw] -translate-x-1/2 -translate-y-1/2',
            'rounded-2xl border border-border bg-panel p-6 shadow-2xl outline-none'
          )}
          onCloseAutoFocus={(e) => {
            e.preventDefault();
            const el = triggerRef?.current;
            if (el) {
              el.focus();
              setTimeout(() => el.focus(), 0);
            }
          }}
          aria-label={t('largetype.label')}
        >
          <div className="flex items-center justify-between pb-3 border-b border-border">
            <D.Title className="text-base font-semibold capitalize">
              {title}
            </D.Title>
            <D.Close
              aria-label={t('largetype.close')}
              className="rounded-lg p-1.5 text-muted hover:bg-hover hover:text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            >
              <X size={18} />
            </D.Close>
          </div>
          <D.Description className="sr-only">
            {title}
          </D.Description>
          <div className="mt-4 max-h-[75vh] overflow-y-auto">
            <div className="flex flex-wrap items-center justify-center gap-4 py-2" data-testid="large-type-grid">
              {groups.map((group, idx) => (
                <div
                  key={idx}
                  data-testid="large-type-group"
                  className="flex items-center gap-1.5 rounded-xl bg-hover/40 p-2 border border-border/40"
                >
                  {group.map((item) => (
                    <div
                      key={item.pos}
                      data-testid="large-type-char"
                      data-kind={item.kind}
                      data-position={item.pos}
                      className="flex flex-col items-center justify-between min-w-[38px] min-h-[58px] px-1 py-1.5 rounded-lg border border-border/70 bg-card/75 shadow-sm select-none"
                    >
                      <span
                        className={cn(
                          'font-mono text-3xl font-semibold leading-none select-all',
                          item.kind === 'digit' && 'large-type-digit',
                          item.kind === 'symbol' && 'large-type-symbol',
                          item.kind === 'letter' && 'large-type-letter',
                          colored ? kindClass(item.kind) : 'text-fg'
                        )}
                      >
                        {item.char === ' ' ? '␣' : item.char}
                      </span>
                      <span
                        data-testid="large-type-pos"
                        className="mt-1 font-mono text-[10px] text-muted leading-none select-none"
                      >
                        {item.pos}
                      </span>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </div>
        </D.Content>
      </D.Portal>
    </Dialog>
  );
}

export function LargeTypeModalHost() {
  const modal = useApp((s) => s.modal);
  const closeModal = useApp((s) => s.closeModal);
  if (modal?.id !== 'large-type' || !modal.props) return null;
  const { title, value } = modal.props as { title: string; value: string };
  return (
    <LargeTypeDialog
      open
      onOpenChange={(o) => {
        if (!o) closeModal();
      }}
      title={title}
      value={value}
    />
  );
}

export const feature: FeatureDef = {
  id: 'large-type',
  Modal: LargeTypeModalHost,
};
