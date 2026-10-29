import { useEffect, useRef, useState } from 'react';
import { Dices, Eye, EyeOff, Plus, Trash2, X } from 'lucide-react';
import { api } from '../../api/client';
import type { Field, FieldKind, ItemDetail, ItemInput, Section, VaultInfo } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import { Button, Input, toast } from '../../ui';
import { ItemIcon } from './categories';
import { StrengthMeter } from './FieldView';
import { PasswordGenerator } from '../new-item/PasswordGenerator';
import { NotesEditorField } from './NotesEditorField';
import { errText } from './actions';

const KINDS: FieldKind[] = ['text', 'concealed', 'url', 'email', 'phone', 'date', 'month_year', 'totp', 'multiline'];
const newId = (p: string) => `new_${p}_${Math.random().toString(36).slice(2, 9)}`;

/** Convert a detail into an editable input. Concealed values stay null = "unchanged" unless the user types a new one. */
export function toInput(d: ItemDetail): ItemInput {
  return { vaultId: d.vaultId, category: d.category, title: d.title, sections: structuredClone(d.sections), notes: d.notes, tags: [...d.tags], favorite: d.favorite, url: d.url ?? null };
}
/** New fields get an empty id so the backend assigns one. */
export function normalizeInput(i: ItemInput): ItemInput {
  return { ...i, title: i.title.trim() || 'Untitled', sections: i.sections.map((s) => ({ ...s, id: s.id.startsWith('new_') ? '' : s.id, fields: s.fields.filter((f) => f.label.trim() || f.value).map((f) => ({ ...f, id: f.id.startsWith('new_') ? '' : f.id })) })) };
}

export function ItemEditor({ detail, vaults }: { detail: ItemDetail; vaults: VaultInfo[] }) {
  const t = useT();
  const { setEditing, refreshItems } = useApp();
  const [draft, setDraft] = useState<ItemInput>(() => toInput(detail));
  const [tagText, setTagText] = useState(detail.tags.join(', '));
  const [busy, setBusy] = useState(false);
  const upd = (p: Partial<ItemInput>) => setDraft((d) => ({ ...d, ...p }));
  const updSection = (si: number, p: Partial<Section>) => upd({ sections: draft.sections.map((s, i) => (i === si ? { ...s, ...p } : s)) });
  const updField = (si: number, fi: number, p: Partial<Field>) => updSection(si, { fields: draft.sections[si].fields.map((f, i) => (i === fi ? { ...f, ...p } : f)) });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { if ((e.target as HTMLElement | null)?.closest?.('[data-generator-popover]')) return; setEditing(false); } if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); void save(); } };
    window.addEventListener('keydown', onKey); return () => window.removeEventListener('keydown', onKey);
  });

  async function save() {
    setBusy(true);
    try {
      const input = normalizeInput({ ...draft, tags: tagText.split(',').map((t) => t.trim()).filter(Boolean) });
      await api.updateItem({ itemId: detail.id, input });
      if (input.vaultId !== detail.vaultId) await api.moveItem({ vaultId: detail.vaultId, itemId: detail.id, toVaultId: input.vaultId }).catch(() => {});
      toast('Item saved'); setEditing(false); refreshItems();
    } catch (e) { toast(errText(e), 'danger'); } finally { setBusy(false); }
  }

  return (
    <div className="flex h-full flex-col" data-testid="item-editor">
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-border px-4">
        <span className="text-sm text-muted">Editing</span>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={() => setEditing(false)}>Cancel</Button>
          <Button size="sm" disabled={busy} onClick={save}>{busy ? 'Saving…' : 'Save'}</Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto px-6 py-6">
        <div className="mx-auto max-w-3xl space-y-4">
          <div className="flex items-center gap-4">
            <ItemIcon item={{ ...detail, title: draft.title || detail.title }} size={48} />
            <Input aria-label="Title" className="h-11 flex-1 text-lg font-semibold" value={draft.title} onChange={(e) => upd({ title: e.target.value })} />
          </div>
          <label className="block text-xs text-muted">Vault
            <select className="mt-1 block h-8 w-full rounded-md border border-border bg-bg px-2 text-sm text-fg" value={draft.vaultId} onChange={(e) => upd({ vaultId: e.target.value })}>
              {vaults.map((v) => <option key={v.id} value={v.id}>{v.name}</option>)}
            </select>
          </label>

          {draft.sections.map((s, si) => (
            <div key={s.id} className="rounded-lg border border-border bg-card p-3">
              <div className="mb-2 flex items-center gap-2">
                <Input placeholder="Section title (optional)" aria-label="Section title" className="h-7 flex-1 border-transparent bg-transparent text-xs font-semibold uppercase tracking-wider" value={s.title ?? ''} onChange={(e) => updSection(si, { title: e.target.value || null })} />
                {draft.sections.length > 1 && <Button variant="ghost" size="iconSm" aria-label="Remove section" onClick={() => upd({ sections: draft.sections.filter((_, i) => i !== si) })}><X size={13} /></Button>}
              </div>
              <div className="space-y-2">
                {s.fields.map((f, fi) => <FieldEditor key={f.id} field={f} onChange={(p) => updField(si, fi, p)} onRemove={() => updSection(si, { fields: s.fields.filter((_, i) => i !== fi) })} />)}
              </div>
              <Button variant="ghost" size="sm" className="mt-2 text-accent" onClick={() => updSection(si, { fields: [...s.fields, { id: newId('f'), label: 'new field', kind: 'text', value: '' }] })}><Plus size={13} />Add field</Button>
            </div>
          ))}
          <Button variant="outline" size="sm" onClick={() => upd({ sections: [...draft.sections, { id: newId('s'), title: 'New section', fields: [] }] })}><Plus size={13} />Add section</Button>

          <label className="block text-xs text-muted">Website<Input className="mt-1" value={draft.url ?? ''} onChange={(e) => upd({ url: e.target.value || null })} placeholder="https://" /></label>
          <label className="block text-xs text-muted">Tags (comma separated)<Input className="mt-1" value={tagText} onChange={(e) => setTagText(e.target.value)} /></label>
          <NotesEditorField label={t('detail.notes')} value={draft.notes ?? ''} onChange={(notes) => upd({ notes: notes || null })} rows={4} />
        </div>
      </div>
    </div>
  );
}

function FieldEditor({ field, onChange, onRemove }: { field: Field; onChange: (p: Partial<Field>) => void; onRemove: () => void }) {
  const t = useT();
  const secret = field.kind === 'concealed' || field.kind === 'totp';
  const [show, setShow] = useState(false);
  const [score, setScore] = useState<number | null>(null);
  const [genOpen, setGenOpen] = useState(false);
  const fieldInputRef = useRef<HTMLInputElement>(null);
  const closeGen = () => { setGenOpen(false); fieldInputRef.current?.focus(); };
  const onFieldKeyDown = (e: React.KeyboardEvent) => {
    // ⌥⌘G (or ⌥Ctrl+G) while the password field is focused opens the popover.
    if (e.altKey && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'g') { e.preventDefault(); setGenOpen(true); }
  };
  useEffect(() => {
    if (field.kind !== 'concealed' || !field.value) { setScore(null); return; }
    let on = true; api.passwordStrength({ password: field.value }).then((s) => on && setScore(s)).catch(() => {}); return () => { on = false; };
  }, [field.kind, field.value]);
  return (
    <div className="rounded-md border border-border/60 bg-bg/40 p-2" data-testid="field-editor">
      <div className="flex items-center gap-2">
        <Input aria-label="Field label" className="h-7 flex-1 text-xs text-label" value={field.label} onChange={(e) => onChange({ label: e.target.value })} />
        <select aria-label="Field type" className="h-7 rounded-md border border-border bg-bg px-1 text-xs text-fg" value={field.kind} onChange={(e) => onChange({ kind: e.target.value as FieldKind, value: null })}>
          {KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
        </select>
        <Button variant="ghost" size="iconSm" aria-label="Remove field" onClick={onRemove}><Trash2 size={13} /></Button>
      </div>
      <div className="relative mt-1.5">
        <div className="flex items-center gap-1">
          {field.kind === 'multiline' ? (
            <div className="w-full">
              <NotesEditorField
                label={`${field.label} value`}
                hideHeaderLabel
                rows={3}
                value={field.value ?? ''}
                onChange={(v) => onChange({ value: v })}
              />
            </div>
          ) : (
            <Input ref={fieldInputRef} aria-label={`${field.label} value`} type={secret && !show ? 'password' : field.kind === 'date' ? 'date' : 'text'} autoComplete="off"
              placeholder={secret && field.value === null ? 'Unchanged — type to replace' : ''} value={field.value ?? ''} onChange={(e) => onChange({ value: e.target.value })}
              onKeyDown={field.kind === 'concealed' ? onFieldKeyDown : undefined} />
          )}
          {secret && <Button variant="ghost" size="iconSm" aria-label={show ? 'Hide' : 'Show'} onClick={() => setShow(!show)}>{show ? <EyeOff size={13} /> : <Eye size={13} />}</Button>}
          {field.kind === 'concealed' && <Button variant="ghost" size="iconSm" aria-label={t('generator.generatePassword')} onClick={() => setGenOpen((o) => !o)}><Dices size={13} /></Button>}
        </div>
        {field.kind === 'concealed' && (
          <PasswordGenerator
            open={genOpen}
            onOpenChange={(o) => { if (o) setGenOpen(true); else closeGen(); }}
            onUse={(pw) => { onChange({ value: pw }); setShow(true); }}
          />
        )}
      </div>
      {score !== null && <StrengthMeter score={score} />}
    </div>
  );
}
