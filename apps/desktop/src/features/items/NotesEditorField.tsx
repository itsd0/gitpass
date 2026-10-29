import React, { useRef, useState } from 'react';
import { cn } from '../../lib/utils';
import { useT } from '../../i18n';
import { Textarea } from '../../ui';
import { SafeMarkdown } from './SafeMarkdown';

export interface NotesEditorFieldProps {
  value: string;
  onChange: (val: string) => void;
  label: string;
  rows?: number;
  hideHeaderLabel?: boolean;
  className?: string;
}

export function NotesEditorField({
  value,
  onChange,
  label,
  rows = 4,
  hideHeaderLabel = false,
  className,
}: NotesEditorFieldProps) {
  const t = useT();
  const [mode, setMode] = useState<'edit' | 'preview'>('edit');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);

  const toggleMode = (nextMode: 'edit' | 'preview') => {
    setMode(nextMode);
    if (nextMode === 'edit') {
      setTimeout(() => textareaRef.current?.focus(), 0);
    } else {
      setTimeout(() => previewRef.current?.focus(), 0);
    }
  };

  const handleShortcutKeyDown = (e: React.KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'p') {
      e.preventDefault();
      e.stopPropagation();
      toggleMode(mode === 'edit' ? 'preview' : 'edit');
    }
  };

  const handleSegmentKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault();
      toggleMode(mode === 'edit' ? 'preview' : 'edit');
    }
  };

  return (
    <div className={cn('block text-xs text-muted', className)} data-testid="notes-editor-field">
      <div className="flex items-center justify-between gap-2">
        {!hideHeaderLabel ? <span>{label}</span> : <span />}
        <div
          role="radiogroup"
          aria-label={t('editor.notesMode')}
          className="flex rounded-md border border-border p-0.5 bg-bg/50"
        >
          <button
            type="button"
            role="radio"
            aria-checked={mode === 'edit'}
            onClick={() => toggleMode('edit')}
            onKeyDown={handleSegmentKeyDown}
            data-testid="notes-mode-edit"
            className={cn(
              'rounded px-2.5 py-0.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
              mode === 'edit' ? 'bg-accent text-white shadow-sm' : 'text-muted hover:text-fg'
            )}
          >
            {t('editor.edit')}
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={mode === 'preview'}
            onClick={() => toggleMode('preview')}
            onKeyDown={handleSegmentKeyDown}
            data-testid="notes-mode-preview"
            className={cn(
              'rounded px-2.5 py-0.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
              mode === 'preview' ? 'bg-accent text-white shadow-sm' : 'text-muted hover:text-fg'
            )}
          >
            {t('editor.preview')}
          </button>
        </div>
      </div>

      {mode === 'edit' ? (
        <Textarea
          ref={textareaRef}
          aria-label={label}
          rows={rows}
          className="mt-1"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={handleShortcutKeyDown}
        />
      ) : (
        <div
          ref={previewRef}
          tabIndex={0}
          role="region"
          aria-label={`${label} ${t('editor.preview')}`}
          data-testid="notes-preview"
          onKeyDown={handleShortcutKeyDown}
          className="mt-1 min-h-[96px] max-h-[360px] overflow-y-auto rounded-md border border-border bg-card p-3 text-sm text-fg outline-none focus-visible:ring-1 focus-visible:ring-accent"
        >
          {value.trim() ? (
            <SafeMarkdown content={value} />
          ) : (
            <span className="text-xs text-muted italic">—</span>
          )}
        </div>
      )}
    </div>
  );
}
