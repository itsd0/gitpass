import React from 'react';
import { cn } from '../../lib/utils';

export function SettingsSection({
  id,
  title,
  description,
  children,
  className,
}: {
  id?: string;
  title?: string;
  description?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div id={id} data-setting-id={id} className={cn('mb-6 last:mb-0', className)}>
      {title && (
        <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted">
          {title}
        </h3>
      )}
      {description && (
        <div className="mb-3 text-xs leading-relaxed text-muted">{description}</div>
      )}
      <div className="divide-y divide-border rounded-lg border border-border bg-card px-4">
        {children}
      </div>
    </div>
  );
}

export function SettingsRow({
  id,
  label,
  description,
  children,
  highlighted,
  className,
}: {
  id?: string;
  label: React.ReactNode;
  description?: React.ReactNode;
  children: React.ReactNode;
  highlighted?: boolean;
  className?: string;
}) {
  return (
    <div
      id={id}
      data-setting-id={id}
      className={cn(
        'flex items-center justify-between gap-6 py-3.5 transition-colors duration-300 motion-reduce:transition-none',
        highlighted && 'rounded-md bg-accent/15 px-2 ring-2 ring-accent',
        className
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium text-fg">{label}</div>
        {description && (
          <div className="mt-0.5 text-xs leading-relaxed text-muted">{description}</div>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
