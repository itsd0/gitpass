import * as React from 'react';
import { Tooltip } from '../../ui';
import { cn } from '../../lib/utils';

// True when the sidebar is collapsed to its icon-only rail.
export const RailContext = React.createContext(false);


export const NavItem = React.forwardRef<
  HTMLButtonElement,
  {
    active?: boolean;
    icon: React.ReactNode;
    label: string;
    onClick: () => void;
    right?: React.ReactNode;
    onContextMenu?: (e: React.MouseEvent) => void;
    onKeyDown?: (e: React.KeyboardEvent) => void;
    onDragOver?: (e: React.DragEvent) => void;
    onDragLeave?: (e: React.DragEvent) => void;
    onDrop?: (e: React.DragEvent) => void;
    className?: string;
  }
>(({ active, icon, label, onClick, right, onContextMenu, onKeyDown, onDragOver, onDragLeave, onDrop, className }, ref) => {
  const rail = React.useContext(RailContext);
  if (rail) {
    return (
      <Tooltip content={label} side="right">
        <button
          ref={ref}
          aria-label={label}
          aria-current={active ? 'page' : undefined}
          data-testid="sidebar-rail-item"
          onClick={onClick}
          onContextMenu={onContextMenu}
          onKeyDown={onKeyDown}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
          className={cn(
            'mx-auto flex h-9 w-9 items-center justify-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-accent',
            active ? 'bg-accent/15 text-accent ring-1 ring-inset ring-accent' : 'hover:bg-hover',
            className
          )}
        >
          <span className="flex justify-center" aria-hidden="true">{icon}</span>
        </button>
      </Tooltip>
    );
  }
  return (
    <button
      ref={ref}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onKeyDown={onKeyDown}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      className={cn(
        'group flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-[14px]',
        active ? 'bg-active font-medium' : 'hover:bg-hover',
        className
      )}
    >
      <span className="flex w-4 justify-center">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {right}
    </button>
  );
});
NavItem.displayName = 'NavItem';
