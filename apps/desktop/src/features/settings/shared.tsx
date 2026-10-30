import { useEffect, useState } from 'react';
import { cn } from '../../lib/utils';
import { Input } from '../../ui';
import { SettingsRow } from './SettingsSection';

export const errMsg = (e: unknown) => String((e as Error)?.message ?? e);

export const Seg = <V extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: V;
  options: { v: V; l: string }[];
  onChange: (v: V) => void;
  label: string;
}) => (
  <div role="radiogroup" aria-label={label} className="flex rounded-lg border border-border p-0.5">
    {options.map((o) => (
      <button
        key={o.v}
        role="radio"
        aria-checked={value === o.v}
        onClick={() => onChange(o.v)}
        className={cn(
          'rounded-md px-3 py-1 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
          value === o.v ? 'bg-accent text-white' : 'text-muted hover:text-fg'
        )}
      >
        {o.l}
      </button>
    ))}
  </div>
);

export function NumberSetting({
  id,
  highlighted,
  label,
  description,
  value,
  min,
  max,
  onSave,
}: {
  id?: string;
  highlighted?: boolean;
  label: string;
  description?: string;
  value: number;
  min: number;
  max: number;
  onSave: (n: number) => void;
}) {
  const [v, setV] = useState(String(value));
  useEffect(() => setV(String(value)), [value]);
  const n = Number(v);
  const ok = Number.isInteger(n) && n >= min && n <= max;
  return (
    <SettingsRow id={id} highlighted={highlighted} label={label} description={description}>
      <Input
        aria-label={label}
        type="number"
        min={min}
        max={max}
        value={v}
        className="w-24 focus-visible:ring-accent"
        aria-invalid={!ok}
        onChange={(e) => setV(e.target.value)}
        onBlur={() => ok && n !== value && onSave(n)}
        onKeyDown={(e) => e.key === 'Enter' && ok && n !== value && onSave(n)}
      />
    </SettingsRow>
  );
}

