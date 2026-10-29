import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import {
  detectActiveOperator,
  applySuggestion,
  SearchSuggestions,
} from './SearchSuggestions';
import { Toolbar } from './Toolbar';
import { TooltipProvider } from '../ui/tooltip';
import type { VaultInfo } from '../api/types';

describe('Search query operators suggestion logic', () => {
  it('detects operator under cursor accurately', () => {
    expect(detectActiveOperator('', null)).toBeNull();
    expect(detectActiveOperator('plain search', null)).toBeNull();
    expect(detectActiveOperator('tag:', null)).toEqual({
      op: 'tag',
      prefix: '',
      start: 0,
      end: 4,
      negated: false,
    });
    expect(detectActiveOperator('hello -tag:wo', null)).toEqual({
      op: 'tag',
      prefix: 'wo',
      start: 6,
      end: 13,
      negated: true,
    });
    expect(detectActiveOperator('type:card', null)).toEqual({
      op: 'type',
      prefix: 'card',
      start: 0,
      end: 9,
      negated: false,
    });
    expect(detectActiveOperator('is:fav', null)).toEqual({
      op: 'is',
      prefix: 'fav',
      start: 0,
      end: 6,
      negated: false,
    });
    expect(detectActiveOperator('has:ot', null)).toEqual({
      op: 'has',
      prefix: 'ot',
      start: 0,
      end: 6,
      negated: false,
    });
    expect(detectActiveOperator('vault:Per', null)).toEqual({
      op: 'vault',
      prefix: 'Per',
      start: 0,
      end: 9,
      negated: false,
    });
    expect(detectActiveOperator('foo:bar', null)).toBeNull();
  });

  it('applies suggestions with correct formatting and quoting', () => {
    const activeTag = detectActiveOperator('tag:wo', null)!;
    expect(applySuggestion('tag:wo', activeTag, 'work')).toBe('tag:work ');

    const activeTagSpaced = detectActiveOperator('tag:si', null)!;
    expect(applySuggestion('tag:si', activeTagSpaced, 'side project')).toBe('tag:"side project" ');

    const activeNegTag = detectActiveOperator('-tag:ol', null)!;
    expect(applySuggestion('-tag:ol', activeNegTag, 'old')).toBe('-tag:old ');

    const activeType = detectActiveOperator('foo type:lo bar', 11)!;
    expect(applySuggestion('foo type:lo bar', activeType, 'login')).toBe('foo type:login  bar');
  });
});

describe('SearchSuggestions popover component', () => {
  const dummyVaults: VaultInfo[] = [
    {
      id: 'v1',
      name: 'Personal',
      description: null,
      itemCount: 2,
      shared: false,
      members: [],
      icon: null,
      color: null,
    },
  ];
  const dummyTags = ['work', 'side project', 'finance'];

  it('renders type suggestions with operator hint and highlights selected item with accent', () => {
    const onSelect = vi.fn();
    const active = detectActiveOperator('type:', null)!;

    render(
      <SearchSuggestions
        active={active}
        tags={dummyTags}
        vaults={dummyVaults}
        selectedIndex={1}
        onSelect={onSelect}
      />
    );

    const popover = screen.getByTestId('search-suggestions');
    expect(popover).toBeInTheDocument();

    const loginOption = screen.getByTestId('search-suggestion-login');
    const cardOption = screen.getByTestId('search-suggestion-card');

    expect(loginOption).toBeInTheDocument();
    expect(cardOption).toBeInTheDocument();

    // Index 1 (card) is selected with orange accent
    expect(cardOption.className).toContain('bg-accent');
    expect(cardOption.className).toContain('text-white');
    expect(cardOption).toHaveAttribute('data-highlighted', 'true');

    // Index 0 (login) is not selected
    expect(loginOption.className).not.toContain('bg-accent');

    // Clicking an option triggers onSelect
    fireEvent.mouseDown(loginOption);
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'login' }));
  });

  it('filters tag suggestions and displays hint', () => {
    const onSelect = vi.fn();
    const active = detectActiveOperator('tag:side', null)!;

    render(
      <SearchSuggestions
        active={active}
        tags={dummyTags}
        vaults={dummyVaults}
        selectedIndex={0}
        onSelect={onSelect}
      />
    );

    expect(screen.getByTestId('search-suggestion-side project')).toBeInTheDocument();
    expect(screen.queryByTestId('search-suggestion-finance')).not.toBeInTheDocument();
  });

  it('renders is: flags (favorite, archived)', () => {
    const active = detectActiveOperator('is:', null)!;

    render(
      <SearchSuggestions
        active={active}
        tags={dummyTags}
        vaults={dummyVaults}
        selectedIndex={0}
        onSelect={vi.fn()}
      />
    );

    expect(screen.getByTestId('search-suggestion-favorite')).toBeInTheDocument();
    expect(screen.getByTestId('search-suggestion-archived')).toBeInTheDocument();
  });

  it('renders has: features (otp, passkey, attachment, url)', () => {
    const active = detectActiveOperator('has:', null)!;

    render(
      <SearchSuggestions
        active={active}
        tags={dummyTags}
        vaults={dummyVaults}
        selectedIndex={0}
        onSelect={vi.fn()}
      />
    );

    expect(screen.getByTestId('search-suggestion-otp')).toBeInTheDocument();
    expect(screen.getByTestId('search-suggestion-passkey')).toBeInTheDocument();
    expect(screen.getByTestId('search-suggestion-attachment')).toBeInTheDocument();
    expect(screen.getByTestId('search-suggestion-url')).toBeInTheDocument();
  });
});

describe('Toolbar search integration', () => {
  it('shows suggestion popover when typing operator and allows keyboard completion', () => {
    render(
      <TooltipProvider>
        <Toolbar />
      </TooltipProvider>
    );
    const input = screen.getByRole('textbox', { name: /search/i });

    fireEvent.change(input, { target: { value: 'has:' } });

    const popover = screen.getByTestId('search-suggestions');
    expect(popover).toBeInTheDocument();

    // Default first suggestion selected (otp)
    const otpOption = screen.getByTestId('search-suggestion-otp');
    expect(otpOption.className).toContain('bg-accent');

    // Arrow down selects passkey
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    const passkeyOption = screen.getByTestId('search-suggestion-passkey');
    expect(passkeyOption.className).toContain('bg-accent');

    // Enter completes passkey into the search input
    fireEvent.keyDown(input, { key: 'Enter' });
    expect((input as HTMLInputElement).value).toBe('has:passkey ');

    // Popover is dismissed after completion
    expect(screen.queryByTestId('search-suggestions')).not.toBeInTheDocument();
  });

  it('closes suggestion popover on Escape', () => {
    render(
      <TooltipProvider>
        <Toolbar />
      </TooltipProvider>
    );
    const input = screen.getByRole('textbox', { name: /search/i });

    fireEvent.change(input, { target: { value: 'type:' } });
    expect(screen.getByTestId('search-suggestions')).toBeInTheDocument();

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByTestId('search-suggestions')).not.toBeInTheDocument();
  });
});
