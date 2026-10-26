import * as React from 'react';
import * as S from '@radix-ui/react-scroll-area';
import { cn } from '../lib/utils';
export const ScrollArea = React.forwardRef<HTMLDivElement, React.ComponentProps<typeof S.Root> & { viewportRef?: React.Ref<HTMLDivElement> }>(({ className, children, viewportRef, ...p }, ref) => (
  <S.Root ref={ref} className={cn('relative overflow-hidden', className)} {...p}>
    <S.Viewport ref={viewportRef} className="h-full w-full">{children}</S.Viewport>
    <S.Scrollbar orientation="vertical" className="flex w-2 touch-none select-none p-0.5"><S.Thumb className="relative flex-1 rounded-full bg-active" /></S.Scrollbar>
  </S.Root>
));
ScrollArea.displayName = 'ScrollArea';
