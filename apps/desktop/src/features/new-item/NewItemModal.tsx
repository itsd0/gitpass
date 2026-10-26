import { useState } from 'react';
import { ChevronDown, Download, Globe, Search } from 'lucide-react';
import type { ItemCategory } from '../../api/types';
import { Modal, Input, Separator } from '../../ui';
import { useApp } from '../../lib/store';
import { domainSuggestion, filterCategories } from './categories';
import { CategoryIcon } from './CategoryIcon';
import { CreateForm } from './CreateForm';
import { ImportWizard } from './ImportWizard';
type View = { kind: 'pick' } | { kind: 'form'; category: ItemCategory; initial?: { title?: string; url?: string } } | { kind: 'import' };
export interface NewItemModalProps { category?: ItemCategory; vaultId?: string; view?: 'import'; }
export function NewItemModal(props: NewItemModalProps) {
  const { closeModal } = useApp();
  const [view, setView] = useState<View>(props.view === 'import' ? { kind: 'import' } : props.category ? { kind: 'form', category: props.category } : { kind: 'pick' });
  const [q, setQ] = useState(''); const [more, setMore] = useState(false);
  const cats = filterCategories(q); const suggestion = domainSuggestion(q);
  const visible = q || more ? cats : cats.filter(c => c.primary);
  return (<Modal open onOpenChange={o => !o && closeModal()} title={view.kind === 'pick' ? 'What would you like to add?' : view.kind === 'import' ? 'Import data' : 'New item'} className="w-[480px]">
    {view.kind === 'pick' && (<div>
      <div className="relative mx-auto mb-5 max-w-xs"><Search size={14} className="absolute left-3 top-2.5 text-muted" />
        <Input autoFocus aria-label="Search for a website or category" placeholder="Search for a website or category" className="pl-8" value={q} onChange={e => setQ(e.target.value)} /></div>
      {suggestion && (<button className="mb-3 flex w-full items-center gap-3 rounded-lg border border-accent/50 bg-accent/10 p-3 text-left hover:bg-accent/20" onClick={() => setView({ kind: 'form', category: 'login', initial: suggestion })}>
        <Globe size={20} className="text-accent" /><div><div className="text-sm font-medium">Create Login for {suggestion.title}</div><div className="text-xs text-muted">{suggestion.url}</div></div></button>)}
      <div className="grid grid-cols-3 gap-2">{visible.map(c => (
        <button key={c.id} data-testid={`tile-${c.id}`} onClick={() => setView({ kind: 'form', category: c.id, initial: suggestion && c.id === 'login' ? suggestion : undefined })}
          className="flex h-[74px] flex-col justify-between rounded-lg border border-border bg-panel p-2.5 text-left text-sm hover:border-border hover:bg-hover">
          <CategoryIcon category={c.id} size={36} /><span>{c.label}</span></button>))}</div>
      {!q && !more && (<button className="mt-2 flex w-full items-center justify-between rounded-lg border border-border bg-panel px-3 py-3 text-sm hover:bg-hover" onClick={() => setMore(true)}>
        More categories <span className="flex items-center gap-1"><CategoryIcon category="ssh_key" size={20} /><CategoryIcon category="wifi" size={20} /><ChevronDown size={14} /></span></button>)}
      {visible.length === 0 && !suggestion && <p className="py-6 text-center text-sm text-muted">No categories match “{q}”.</p>}
      <Separator className="my-3" />
      <button className="mx-auto flex items-center gap-1.5 text-sm text-accent hover:underline" onClick={() => setView({ kind: 'import' })}><Download size={14} /> Migrate data into gitpass</button></div>)}
    {view.kind === 'form' && <CreateForm category={view.category} initial={view.initial} vaultId={props.vaultId} onBack={() => setView({ kind: 'pick' })} onDone={closeModal} />}
    {view.kind === 'import' && <ImportWizard onBack={() => setView({ kind: 'pick' })} onDone={closeModal} />}
  </Modal>);
}

/** Registry modal host: AppShell renders `<Modal />` without props, so read them from the store. */
export function NewItemModalHost() {
  const props = useApp((s) => s.modal?.props) as NewItemModalProps | undefined;
  return <NewItemModal {...(props ?? {})} />;
}
