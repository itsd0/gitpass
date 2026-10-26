import type * as React from "react";
import * as S from '@radix-ui/react-separator';
import { cn } from '../lib/utils';
export const Separator = ({ className, orientation = 'horizontal', ...p }: React.ComponentProps<typeof S.Root>) =>
  <S.Root orientation={orientation} className={cn('shrink-0 bg-border', orientation === 'horizontal' ? 'h-px w-full' : 'h-full w-px', className)} {...p} />;
