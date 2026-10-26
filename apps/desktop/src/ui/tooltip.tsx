import * as React from 'react';
import * as T from '@radix-ui/react-tooltip';
export const TooltipProvider = T.Provider;
export function Tooltip({ content, children, side = 'top' }: { content: React.ReactNode; children: React.ReactNode; side?: 'top' | 'bottom' | 'left' | 'right' }) {
  return (
    <T.Root delayDuration={300}>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal><T.Content side={side} sideOffset={4} className="z-50 rounded bg-fg px-2 py-1 text-xs text-bg shadow">{content}</T.Content></T.Portal>
    </T.Root>
  );
}
