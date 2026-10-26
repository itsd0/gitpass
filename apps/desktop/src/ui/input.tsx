import * as React from 'react';
import { cn } from '../lib/utils';
export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(({ className, ...p }, ref) =>
  <input ref={ref} className={cn('h-8 w-full rounded-md border border-border bg-bg px-2.5 text-sm text-fg placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-accent', className)} {...p} />);
Input.displayName = 'Input';
export const Textarea = React.forwardRef<HTMLTextAreaElement, React.TextareaHTMLAttributes<HTMLTextAreaElement>>(({ className, ...p }, ref) =>
  <textarea ref={ref} className={cn('w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-sm text-fg placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/50', className)} {...p} />);
Textarea.displayName = 'Textarea';
