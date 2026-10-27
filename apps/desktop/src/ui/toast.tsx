import * as T from '@radix-ui/react-toast';
import { create } from 'zustand';
interface ToastMsg { id: number; title: string; tone?: 'default' | 'danger' }
export const useToasts = create<{ list: ToastMsg[]; push: (t: Omit<ToastMsg, 'id'>) => void; remove: (id: number) => void }>((set) => ({
  list: [], push: (t) => set((s) => ({ list: [...s.list, { ...t, id: Date.now() + Math.random() }] })), remove: (id) => set((s) => ({ list: s.list.filter((x) => x.id !== id) })),
}));
// A locked vault is shown as the unlock screen, never as an error toast.
export const toast = (title: string, tone?: 'default' | 'danger') => { if (/vault (is )?locked/i.test(String(title))) return; useToasts.getState().push({ title, tone }); };
export function Toaster() {
  const { list, remove } = useToasts();
  return (
    <T.Provider swipeDirection="right" duration={2500}>
      {list.map((t) => (
        <T.Root key={t.id} onOpenChange={(o) => !o && remove(t.id)} className={`rounded-lg border border-border px-4 py-2.5 shadow-xl ${t.tone === 'danger' ? 'bg-danger text-white' : 'bg-panel text-fg'}`}>
          <T.Title className="text-sm">{t.title}</T.Title>
        </T.Root>
      ))}
      <T.Viewport className="fixed bottom-4 right-4 z-[100] flex w-80 flex-col gap-2 outline-none" />
    </T.Provider>
  );
}
