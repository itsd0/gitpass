import * as React from 'react';
import { cn } from '../lib/utils';
export const Badge = ({ className, tone = 'default', ...p }: React.HTMLAttributes<HTMLSpanElement> & { tone?: 'default' | 'accent' | 'danger' | 'ok' | 'warn' }) =>
  <span className={cn('inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium',
    { default: 'bg-hover text-fg', accent: 'bg-accent/15 text-accent', danger: 'bg-danger/15 text-danger', ok: 'bg-ok/15 text-ok', warn: 'bg-warn/15 text-warn' }[tone], className)} {...p} />;
