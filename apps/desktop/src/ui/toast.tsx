import * as T from '@radix-ui/react-toast';
import { create } from 'zustand';

export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
  altText?: string;
}

export interface ToastOptions {
  tone?: 'default' | 'danger';
  duration?: number;
  action?: ToastAction;
}

export interface ToastMsg {
  id: number;
  title: string;
  tone?: 'default' | 'danger';
  duration?: number;
  action?: ToastAction;
}

export const useToasts = create<{
  list: ToastMsg[];
  push: (t: Omit<ToastMsg, 'id'>) => void;
  remove: (id: number) => void;
}>((set) => ({
  list: [],
  push: (t) => set((s) => ({ list: [...s.list, { ...t, id: Date.now() + Math.random() }] })),
  remove: (id) => set((s) => ({ list: s.list.filter((x) => x.id !== id) })),
}));

// A locked vault is shown as the unlock screen, never as an error toast.
export const toast = (
  title: string,
  toneOrOpts?: 'default' | 'danger' | ToastOptions
) => {
  if (/vault (is )?locked/i.test(String(title))) return;
  const opts: ToastOptions =
    typeof toneOrOpts === 'string' ? { tone: toneOrOpts } : (toneOrOpts ?? {});
  useToasts.getState().push({ title, ...opts });
};

export function Toaster() {
  const { list, remove } = useToasts();
  return (
    <T.Provider swipeDirection="right" duration={2500}>
      {list.map((t) => (
        <T.Root
          key={t.id}
          duration={t.duration}
          onOpenChange={(o) => !o && remove(t.id)}
          className={`flex items-center justify-between gap-3 rounded-lg border border-border px-4 py-2.5 shadow-xl ${
            t.tone === 'danger' ? 'bg-danger text-white' : 'bg-panel text-fg'
          }`}
        >
          <T.Title className="text-sm">{t.title}</T.Title>
          {t.action && (
            <T.Action
              altText={t.action.altText ?? t.action.label}
              asChild
              onClick={() => {
                remove(t.id);
                void t.action!.onClick();
              }}
            >
              <button
                type="button"
                className="shrink-0 rounded px-2 py-0.5 text-xs font-semibold uppercase tracking-wider text-accent hover:underline focus:outline-none"
              >
                {t.action.label}
              </button>
            </T.Action>
          )}
        </T.Root>
      ))}
      <T.Viewport className="fixed bottom-4 right-4 z-[100] flex w-80 flex-col gap-2 outline-none" />
    </T.Provider>
  );
}
