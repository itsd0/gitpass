import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Dices, Eye, EyeOff, GripVertical, Plus, Trash2, X } from 'lucide-react';
import { api } from '../../api/client';
import type { Field, FieldKind, ItemDetail, ItemInput, Section, VaultInfo } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Input, toast } from '../../ui';
import { ItemIcon } from './categories';
import { StrengthMeter } from './FieldView';
import { PasswordGenerator } from '../new-item/PasswordGenerator';
import { NotesEditorField } from './NotesEditorField';
import { errText } from './actions';
import { FIELD_TYPES } from './fieldTypes';
import { insertAt, moveItem } from './editorMove';
import { parseTotp, totpCode, totpRemaining } from './totpPreview';

type Drag = { type: 'field'; si: number; fi: number } | { type: 'section'; si: number } | null;
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
  const [focusId, setFocusId] = useState<string | null>(null);
  const [drag, setDrag] = useState<Drag>(null);
  const [over, setOver] = useState<string | null>(null);
  const [announce, setAnnounce] = useState('');
  const keepFocus = useRef<HTMLElement | null>(null);
  const skipMenuRefocus = useRef(false);
  // A keyed reorder moves DOM nodes, which can drop focus: put it back on the element that had it.
  useLayoutEffect(() => {
    const el = keepFocus.current; keepFocus.current = null;
    if (el && el.isConnected && document.activeElement !== el) el.focus();
  });
  const upd = (p: Partial<ItemInput>) => setDraft((d) => ({ ...d, ...p }));
  const updSection = (si: number, p: Partial<Section>) => upd({ sections: draft.sections.map((s, i) => (i === si ? { ...s, ...p } : s)) });
  const updField = (si: number, fi: number, p: Partial<Field>) => updSection(si, { fields: draft.sections[si].fields.map((f, i) => (i === fi ? { ...f, ...p } : f)) });

  const moveField = (si: number, fi: number, to: number) => {
    const fields = draft.sections[si].fields;
    const dest = Math.max(0, Math.min(fields.length - 1, to));
    if (dest === fi) return;
    keepFocus.current = document.activeElement as HTMLElement | null;
    updSection(si, { fields: moveItem(fields, fi, dest) });
    setAnnounce(t('editor.moved', { n: dest + 1, total: fields.length }));
  };
  const moveSection = (si: number, to: number) => {
    const dest = Math.max(0, Math.min(draft.sections.length - 1, to));
    if (dest === si) return;
    keepFocus.current = document.activeElement as HTMLElement | null;
    upd({ sections: moveItem(draft.sections, si, dest) });
    setAnnounce(t('editor.moved', { n: dest + 1, total: draft.sections.length }));
  };
  const addField = (kind: FieldKind, label: string, si: number) => {
    const id = newId('f');
    updSection(si, { fields: [...draft.sections[si].fields, { id, label, kind, value: '' }] });
    setFocusId(id); skipMenuRefocus.current = true;
  };
  const removeField = (si: number, fi: number) => {
    const sec = draft.sections[si]; const field = sec.fields[fi];
    updSection(si, { fields: sec.fields.filter((_, i) => i !== fi) });
    toast(t('editor.fieldRemoved'), { duration: 6000, action: { label: t('editor.undo'), onClick: () => setDraft((d) => ({ ...d, sections: d.sections.map((s) => (s.id === sec.id ? { ...s, fields: insertAt(s.fields, fi, field) } : s)) })) } });
  };
  const removeSection = (si: number) => {
    const sec = draft.sections[si];
    upd({ sections: draft.sections.filter((_, i) => i !== si) });
    toast(t('editor.sectionRemoved'), { duration: 6000, action: { label: t('editor.undo'), onClick: () => setDraft((d) => ({ ...d, sections: insertAt(d.sections, si, sec) })) } });
  };
  const altMove = (e: React.KeyboardEvent, move: (delta: number) => void) => {
    if (!e.altKey || e.metaKey || e.ctrlKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return;
    e.preventDefault(); e.stopPropagation(); move(e.key === 'ArrowUp' ? -1 : 1);
  };
  const endDrag = () => { setDrag(null); setOver(null); };

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
      toast(t('editor.saved')); setEditing(false); refreshItems();
    } catch (e) { toast(errText(e), 'danger'); } finally { setBusy(false); }
  }

  return (
    <div className="flex h-full flex-col" data-testid="item-editor">
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-border px-4">
        <span className="text-sm text-muted">{t('editor.editing')}</span>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={() => setEditing(false)}>{t('editor.cancel')}</Button>
          <Button size="sm" disabled={busy} onClick={save}>{busy ? t('editor.saving') : t('editor.save')}</Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto px-6 py-6">
        <div className="mx-auto max-w-3xl space-y-4">
          <div className="flex items-center gap-4">
            <ItemIcon item={{ ...detail, title: draft.title || detail.title }} size={48} />
            <Input aria-label={t('editor.title')} className="h-11 flex-1 text-lg font-semibold" value={draft.title} onChange={(e) => upd({ title: e.target.value })} />
          </div>
          <label className="block text-xs text-muted">{t('editor.vault')}
            <select className="mt-1 block h-8 w-full rounded-md border border-border bg-bg px-2 text-sm text-fg" value={draft.vaultId} onChange={(e) => upd({ vaultId: e.target.value })}>
              {vaults.map((v) => <option key={v.id} value={v.id}>{v.name}</option>)}
            </select>
          </label>

          {draft.sections.map((s, si) => {
            const secKey = `s:${si}`;
            const secOver = drag?.type === 'section' && over === secKey && drag.si !== si;
            return (
            <div key={s.id} data-testid="section-editor" data-section-id={s.id}
              className={`rounded-lg border bg-card p-3 ${secOver ? 'border-accent ring-1 ring-accent' : 'border-border'}`}
              onDragOver={(e) => { if (drag?.type !== 'section') return; e.preventDefault(); setOver(secKey); }}
              onDrop={(e) => { if (drag?.type !== 'section') return; e.preventDefault(); moveSection(drag.si, si); endDrag(); }}>
              <div className="mb-2 flex items-center gap-2" onKeyDown={(e) => altMove(e, (d) => moveSection(si, si + d))}>
                {draft.sections.length > 1 && (
                  <button type="button" draggable aria-label={t('editor.dragSection')} title={t('editor.dragSection')} data-testid="section-handle"
                    className="cursor-grab rounded p-0.5 text-muted hover:text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                    onDragStart={(e) => { e.dataTransfer?.setData('text/plain', s.id); setDrag({ type: 'section', si }); }} onDragEnd={endDrag}>
                    <GripVertical size={14} />
                  </button>
                )}
                <Input placeholder={t('editor.sectionTitle')} aria-label={t('editor.sectionTitle')} className="h-7 flex-1 border-transparent bg-transparent text-xs font-semibold uppercase tracking-wider focus-visible:ring-accent" value={s.title ?? ''} onChange={(e) => updSection(si, { title: e.target.value || null })} />
                {draft.sections.length > 1 && <Button variant="ghost" size="iconSm" aria-label={t('editor.removeSection')} onClick={() => removeSection(si)}><X size={13} /></Button>}
              </div>
              <div className="space-y-2">
                {s.fields.map((f, fi) => (
                  <FieldEditor key={f.id} field={f} autoFocusLabel={f.id === focusId} onFocused={() => setFocusId(null)}
                    onChange={(p) => updField(si, fi, p)} onRemove={() => removeField(si, fi)}
                    onKeyDown={(e) => altMove(e, (d) => moveField(si, fi, fi + d))}
                    dropActive={drag?.type === 'field' && drag.si === si && over === `f:${si}:${fi}` && drag.fi !== fi}
                    onDragStart={(e) => { e.stopPropagation(); e.dataTransfer?.setData('text/plain', f.id); setDrag({ type: 'field', si, fi }); }}
                    onDragEnd={endDrag}
                    onDragOver={(e) => { if (drag?.type !== 'field' || drag.si !== si) return; e.preventDefault(); e.stopPropagation(); setOver(`f:${si}:${fi}`); }}
                    onDrop={(e) => { if (drag?.type !== 'field' || drag.si !== si) return; e.preventDefault(); e.stopPropagation(); moveField(si, drag.fi, fi); endDrag(); }} />
                ))}
              </div>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="sm" className="mt-2 text-accent" aria-haspopup="menu"><Plus size={13} />{t('editor.addField')}</Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" onCloseAutoFocus={(e) => { if (skipMenuRefocus.current) { e.preventDefault(); skipMenuRefocus.current = false; } }}>
                  {FIELD_TYPES.map(({ kind, icon: Icon, label }) => (
                    <DropdownMenuItem key={kind} data-kind={kind} onSelect={() => addField(kind, t(label), si)}><Icon size={14} className="text-accent" />{t(label)}</DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
            );
          })}
          <Button variant="outline" size="sm" onClick={() => upd({ sections: [...draft.sections, { id: newId('s'), title: t('editor.newSection'), fields: [] }] })}><Plus size={13} />{t('editor.addSection')}</Button>
          <div role="status" aria-live="polite" className="sr-only" data-testid="editor-announce">{announce}</div>

          <label className="block text-xs text-muted">{t('editor.website')}<Input className="mt-1" value={draft.url ?? ''} onChange={(e) => upd({ url: e.target.value || null })} placeholder="https://" /></label>
          <label className="block text-xs text-muted">{t('editor.tags')}<Input className="mt-1" value={tagText} onChange={(e) => setTagText(e.target.value)} /></label>
          <NotesEditorField label={t('detail.notes')} value={draft.notes ?? ''} onChange={(notes) => upd({ notes: notes || null })} rows={4} />
        </div>
      </div>
    </div>
  );
}

interface FieldEditorProps {
  field: Field; onChange: (p: Partial<Field>) => void; onRemove: () => void;
  autoFocusLabel?: boolean; onFocused?: () => void; dropActive?: boolean;
  onKeyDown?: (e: React.KeyboardEvent) => void;
  onDragStart?: (e: React.DragEvent) => void; onDragEnd?: (e: React.DragEvent) => void;
  onDragOver?: (e: React.DragEvent) => void; onDrop?: (e: React.DragEvent) => void;
}

/** Live code preview for a One-time password field being edited (otpauth:// URI or base32 secret). */
function TotpPreview({ value }: { value: string }) {
  const t = useT();
  const spec = useMemo(() => parseTotp(value), [value]);
  const [now, setNow] = useState(() => Date.now());
  const [code, setCode] = useState('');
  useEffect(() => {
    if (!spec) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [spec]);
  useEffect(() => {
    if (!spec) { setCode(''); return; }
    let on = true;
    totpCode(spec, now).then((c) => on && setCode(c)).catch(() => on && setCode(''));
    return () => { on = false; };
  }, [spec, now]);
  if (!spec) return <p role="alert" className="mt-1 text-xs text-danger" data-testid="totp-error">{t('editor.otpInvalid')}</p>;
  return (
    <p className="mt-1 flex items-center gap-2 text-xs text-muted" data-testid="totp-preview">
      <span>{t('editor.otpPreview')}</span>
      <span className="font-mono text-sm tracking-widest text-accent" data-testid="totp-code">{code}</span>
      <span>{t('editor.otpExpires', { n: totpRemaining(spec, now) })}</span>
    </p>
  );
}

function FieldEditor({ field, onChange, onRemove, autoFocusLabel, onFocused, dropActive, onKeyDown, onDragStart, onDragEnd, onDragOver, onDrop }: FieldEditorProps) {
  const t = useT();
  const secret = field.kind === 'concealed' || field.kind === 'totp';
  const [show, setShow] = useState(false);
  const [score, setScore] = useState<number | null>(null);
  const [genOpen, setGenOpen] = useState(false);
  const fieldInputRef = useRef<HTMLInputElement>(null);
  const labelRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!autoFocusLabel) return;
    // Wait a tick: the "Add field" menu is still closing (and trapping focus) on the commit that mounts this row.
    const id = setTimeout(() => { labelRef.current?.focus(); labelRef.current?.select(); onFocused?.(); }, 0);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
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
    <div className={`rounded-md border bg-bg/40 p-2 focus-within:border-accent ${dropActive ? 'border-accent ring-1 ring-accent' : 'border-border/60'}`} data-testid="field-editor"
      onKeyDown={onKeyDown} onDragOver={onDragOver} onDrop={onDrop}>
      <div className="flex items-center gap-2">
        <button type="button" draggable aria-label={t('editor.dragField')} title={t('editor.dragField')} data-testid="field-handle"
          className="cursor-grab rounded p-0.5 text-muted hover:text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          onDragStart={onDragStart} onDragEnd={onDragEnd}>
          <GripVertical size={14} />
        </button>
        <Input ref={labelRef} aria-label={t('editor.fieldLabel')} className="h-7 flex-1 text-xs text-label" value={field.label} onChange={(e) => onChange({ label: e.target.value })} />
        <select aria-label={t('editor.fieldType')} className="h-7 rounded-md border border-border bg-bg px-1 text-xs text-fg" value={field.kind} onChange={(e) => onChange({ kind: e.target.value as FieldKind, value: null })}>
          {FIELD_TYPES.map((ft) => <option key={ft.kind} value={ft.kind}>{t(ft.label)}</option>)}
        </select>
        <Button variant="ghost" size="iconSm" aria-label={t('editor.removeField')} onClick={onRemove}><Trash2 size={13} /></Button>
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
            <Input ref={fieldInputRef} aria-label={`${field.label} value`} type={secret && !show ? 'password' : field.kind === 'date' ? 'date' : 'text'} autoComplete="off" aria-invalid={field.kind === 'totp' && !!field.value && !parseTotp(field.value) ? true : undefined}
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
      {field.kind === 'totp' && !!field.value && <TotpPreview value={field.value} />}
      {score !== null && <StrengthMeter score={score} />}
    </div>
  );
}
