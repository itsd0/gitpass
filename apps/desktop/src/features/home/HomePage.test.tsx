import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../api/client';
import type { Onboarding, VaultInfo } from '../../api/types';
import { HomePage } from './HomePage';
import { Toaster, TooltipProvider, useToasts } from '../../ui';

const dummyVaults: VaultInfo[] = [
  { id: 'v1', name: 'Personal', description: null, itemCount: 5, shared: false, members: [], icon: 'vault', color: '#f97316' },
  { id: 'v2', name: 'Work', description: 'Work logins', itemCount: 10, shared: true, members: [], icon: 'briefcase', color: '#3b82f6' },
];

describe('HomePage', () => {
  let onbState: Onboarding;

  beforeEach(() => {
    onbState = {
      extensionInstalled: false,
      emergencyKitSaved: false,
      repoConnected: false,
      imported: false,
      dismissed: false,
    };
    vi.spyOn(api, 'listVaults').mockResolvedValue(dummyVaults);
    vi.spyOn(api, 'onboarding').mockImplementation(async () => ({ ...onbState }));
    vi.spyOn(api, 'setOnboarding').mockImplementation(async ({ patch }) => {
      onbState = { ...onbState, ...patch };
      return { ...onbState };
    });
    vi.spyOn(api, 'syncStatus').mockResolvedValue({
      remote: null,
      lastSync: null,
      ahead: 0,
      behind: 0,
      dirty: false,
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    useToasts.setState({ list: [] });
  });

  it('renders root container with h-full and overflow-y-auto to allow scrolling to vaults cards', async () => {
    render(
      <TooltipProvider>
        <HomePage />
      </TooltipProvider>
    );

    const scrollContainer = await screen.findByTestId('home-scroll');
    expect(scrollContainer.className).toContain('h-full');
    expect(scrollContainer.className).toContain('overflow-y-auto');

    expect(await screen.findByRole('region', { name: 'Let’s get started' })).toBeTruthy();
    expect(await screen.findByRole('region', { name: 'Vaults' })).toBeTruthy();
    expect(screen.getByText('All Vaults')).toBeTruthy();
    expect(screen.getByText('Personal')).toBeTruthy();
    expect(screen.getByText('Work')).toBeTruthy();
  });

  it('renders dismiss button as a 32px icon button with type="button", focus ring and aria-label', async () => {
    render(
      <TooltipProvider>
        <HomePage />
      </TooltipProvider>
    );

    const dismissBtn = await screen.findByRole('button', { name: 'Dismiss' });
    expect(dismissBtn).toBeTruthy();
    expect(dismissBtn).toHaveAttribute('type', 'button');
    expect(dismissBtn.className).toContain('h-8');
    expect(dismissBtn.className).toContain('w-8');
    expect(dismissBtn.className).toContain('focus-visible:ring-accent');
  });

  it('clicking dismiss hides the onboarding card and triggers an undo toast which restores it on click', async () => {
    render(
      <TooltipProvider>
        <HomePage />
        <Toaster />
      </TooltipProvider>
    );

    const dismissBtn = await screen.findByRole('button', { name: 'Dismiss' });
    expect(screen.getByRole('region', { name: 'Let’s get started' })).toBeTruthy();

    fireEvent.click(dismissBtn);

    await waitFor(() => {
      expect(screen.queryByRole('region', { name: 'Let’s get started' })).toBeNull();
    });
    expect(api.setOnboarding).toHaveBeenCalledWith({ patch: { dismissed: true } });

    const toastTitle = await screen.findByText('Setup guide hidden');
    expect(toastTitle).toBeTruthy();
    const undoBtn = screen.getByRole('button', { name: 'Undo' });
    expect(undoBtn).toBeTruthy();

    fireEvent.click(undoBtn);

    await waitFor(() => {
      expect(screen.getByRole('region', { name: 'Let’s get started' })).toBeTruthy();
    });
    expect(api.setOnboarding).toHaveBeenCalledWith({ patch: { dismissed: false } });
    expect(await screen.findByText('Setup guide restored')).toBeTruthy();
  });

  it('does not render onboarding card when dismissed is initially true', async () => {
    onbState.dismissed = true;
    render(
      <TooltipProvider>
        <HomePage />
      </TooltipProvider>
    );

    expect(await screen.findByRole('region', { name: 'Vaults' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'Let’s get started' })).toBeNull();
  });

  it('extension button in onboarding card has type="button" and focus ring', async () => {
    render(
      <TooltipProvider>
        <HomePage />
      </TooltipProvider>
    );

    const extBtn = await screen.findByRole('button', { name: /Get it now/i });
    expect(extBtn).toHaveAttribute('type', 'button');
    expect(extBtn.className).toContain('focus-visible:ring-accent');

    fireEvent.click(extBtn);
    expect(api.setOnboarding).toHaveBeenCalledWith({ patch: { extensionInstalled: true } });
  });
});
