import * as React from 'react';
import { cn } from '../lib/utils';
export const Kbd = ({ className, ...p }: React.HTMLAttributes<HTMLElement>) =>
  <kbd className={cn('inline-flex h-5 min-w-5 items-center justify-center rounded border border-border bg-hover px-1 font-sans text-[11px] text-muted', className)} {...p} />;
