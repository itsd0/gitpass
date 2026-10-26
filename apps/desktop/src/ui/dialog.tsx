import * as React from 'react';
import * as D from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { cn } from '../lib/utils';
export const Dialog = D.Root; export const DialogTrigger = D.Trigger; export const DialogClose = D.Close;
export function DialogContent({ className, children, title, description, ...p }: React.ComponentProps<typeof D.Content> & { title: string; description?: string }) {
  return (
    <D.Portal>
      <D.Overlay className="fixed inset-0 z-40 bg-black/50" />
      <D.Content className={cn('fixed left-1/2 top-1/2 z-50 w-[440px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-panel p-5 shadow-2xl', className)} {...p}>
        <D.Title className="text-base font-semibold">{title}</D.Title>
        {description ? <D.Description className="mt-1 text-muted">{description}</D.Description> : <D.Description className="sr-only">{title}</D.Description>}
        <div className="mt-4">{children}</div>
        <D.Close aria-label="Close" className="absolute right-3 top-3 rounded p-1 text-muted hover:bg-hover"><X size={16} /></D.Close>
      </D.Content>
    </D.Portal>
  );
}

/** Convenience controlled dialog built on the primitives above (title + optional description/footer). */
export function Modal({ open, onOpenChange, title, description, footer, wide, className, children }: {
  open: boolean; onOpenChange: (o: boolean) => void; title: string; description?: string; footer?: React.ReactNode;
  wide?: boolean; className?: string; children?: React.ReactNode;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={title} description={description} className={cn('max-h-[88vh] overflow-y-auto', wide ? 'w-[640px]' : undefined, className)}>
        {children}
        {footer && <div className="mt-5 flex justify-end gap-2">{footer}</div>}
      </DialogContent>
    </Dialog>
  );
}
