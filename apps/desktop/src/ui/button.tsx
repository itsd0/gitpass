import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../lib/utils';
export const buttonVariants = cva(
  'inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:opacity-50 disabled:pointer-events-none',
  { variants: {
      variant: { default: 'bg-accent text-white hover:bg-accent/90', ghost: 'text-fg hover:bg-hover', outline: 'border border-border bg-transparent hover:bg-hover', danger: 'bg-danger text-white hover:bg-danger/90', subtle: 'bg-hover text-fg hover:bg-active', link: 'text-accent hover:underline px-0' },
      size: { sm: 'h-7 px-2 text-xs', md: 'h-8 px-3', lg: 'h-10 px-4', icon: 'h-8 w-8', iconSm: 'h-6 w-6' } },
    defaultVariants: { variant: 'default', size: 'md' } });
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}
export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(({ className, variant, size, ...p }, ref) =>
  <button ref={ref} className={cn(buttonVariants({ variant, size }), className)} {...p} />);
Button.displayName = 'Button';
