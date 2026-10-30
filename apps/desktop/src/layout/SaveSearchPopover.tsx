import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useT } from '../i18n';

export interface SaveSearchPopoverProps {
  initialName: string;
  onSave: (name: string) => void;
  onCancel: () => void;
}

export function SaveSearchPopover({
  initialName,
  onSave,
  onCancel,
}: SaveSearchPopoverProps) {
  const [name, setName] = useState(initialName);
  const inputRef = useRef<HTMLInputElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const t = useT();

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  useEffect(() => {
    const handleOutside = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        onCancel();
      }
    };
    document.addEventListener('mousedown', handleOutside);
    return () => document.removeEventListener('mousedown', handleOutside);
  }, [onCancel]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    onSave(trimmed);
  };

  return (
    <div
      ref={popoverRef}
      role="dialog"
      aria-label={t('savedSearch.save')}
      data-testid="save-search-popover"
      className="absolute right-0 top-full z-50 mt-1.5 w-72 rounded-lg border border-border bg-panel p-3 shadow-xl backdrop-blur motion-safe:animate-[fadeIn_.12s_ease-out]"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          onCancel();
        }
      }}
    >
      <form onSubmit={handleSubmit} className="flex flex-col gap-2.5">
        <label className="text-xs font-semibold text-muted">
          {t('savedSearch.name')}
        </label>
        <input
          ref={inputRef}
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t('savedSearch.namePlaceholder')}
          aria-label={t('savedSearch.name')}
          className="h-8 rounded-md border border-border bg-bg px-2.5 text-sm text-fg outline-none focus:border-accent focus:ring-1 focus:ring-accent"
        />
        <div className="flex justify-end gap-1.5 pt-1">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onCancel}
          >
            {t('common.cancel')}
          </Button>
          <Button
            type="submit"
            size="sm"
            disabled={!name.trim()}
          >
            {t('savedSearch.save')}
          </Button>
        </div>
      </form>
    </div>
  );
}
