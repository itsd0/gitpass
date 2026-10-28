import * as React from 'react';
import * as M from '@radix-ui/react-dropdown-menu';
import { ChevronRight } from 'lucide-react';
import { cn } from '../lib/utils';
export const DropdownMenu = M.Root; export const DropdownMenuTrigger = M.Trigger; export const DropdownMenuSub = M.Sub;
const content = 'z-50 min-w-[180px] rounded-lg border border-border bg-panel p-1 shadow-xl text-sm';
export const DropdownMenuContent = ({ className, ...p }: React.ComponentProps<typeof M.Content>) =>
  <M.Portal><M.Content sideOffset={4} className={cn(content, className)} {...p} /></M.Portal>;
export const DropdownMenuItem = ({ className, danger, ...p }: React.ComponentProps<typeof M.Item> & { danger?: boolean }) =>
  <M.Item className={cn('flex cursor-default select-none items-center gap-2 rounded px-2 py-1.5 outline-none data-[highlighted]:bg-hover data-[disabled]:opacity-50', danger && 'text-danger data-[highlighted]:text-danger data-[highlighted]:bg-danger/10 dark:text-danger dark:data-[highlighted]:text-danger dark:data-[highlighted]:bg-danger/20', className)} {...p} />;
export const DropdownMenuSeparator = () => <M.Separator className="my-1 h-px bg-border" />;
export const DropdownMenuLabel = ({ className, ...p }: React.ComponentProps<typeof M.Label>) => <M.Label className={cn('px-2 py-1 text-xs text-muted', className)} {...p} />;
export const DropdownMenuSubTrigger = ({ className, children, ...p }: React.ComponentProps<typeof M.SubTrigger>) =>
  <M.SubTrigger className={cn('flex cursor-default select-none items-center gap-2 rounded px-2 py-1.5 outline-none data-[highlighted]:bg-hover data-[state=open]:bg-hover', className)} {...p}>{children}<ChevronRight size={14} className="ml-auto" /></M.SubTrigger>;
export const DropdownMenuSubContent = ({ className, ...p }: React.ComponentProps<typeof M.SubContent>) =>
  <M.Portal><M.SubContent className={cn(content, className)} {...p} /></M.Portal>;
