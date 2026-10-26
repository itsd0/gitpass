import * as A from '@radix-ui/react-avatar';
import { cn } from '../lib/utils';
const palette = ['#6366f1', '#0ea5e9', '#ec4899', '#22c55e', '#f59e0b', '#ef4444', '#14b8a6', '#a855f7'];
export const colorFor = (s: string) => palette[[...s].reduce((a, c) => a + c.charCodeAt(0), 0) % palette.length];
export function Avatar({ name, src, size = 28, className, color }: { name: string; src?: string | null; size?: number; className?: string; color?: string | null }) {
  return (
    <A.Root className={cn('inline-flex shrink-0 select-none items-center justify-center overflow-hidden rounded-md font-semibold text-white', className)}
      style={{ width: size, height: size, background: color ?? colorFor(name), fontSize: size * 0.45 }}>
      {src && <A.Image src={src} alt="" className="h-full w-full object-cover" />}
      <A.Fallback delayMs={0}>{(name.trim()[0] ?? '?').toUpperCase()}</A.Fallback>
    </A.Root>
  );
}
