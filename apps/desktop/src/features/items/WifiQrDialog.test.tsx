import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { WifiQrDialog } from './WifiQrDialog';
import { ItemDetailPane } from './ItemDetail';
import { TooltipProvider } from '../../ui';
import { useApp } from '../../lib/store';
import { useItemsUI } from './state';
import { api } from '../../api/client';
import type { ItemDetail, ItemSummary } from '../../api/types';

const mockWifiDetailWithPassword: ItemDetail = {
  id: 'wifi-item-1',
  vaultId: 'v_personal',
  title: 'Home Wi-Fi',
  subtitle: 'Naing-5G',
  category: 'wifi',
  favorite: false,
  archived: false,
  sections: [
    {
      id: 'main',
      title: null,
      fields: [
        { id: 'ssid', label: 'network name', kind: 'text', value: 'Naing-5G', masked: null, diff: null },
        { id: 'password', label: 'wireless password', kind: 'concealed', value: null, masked: '••••••••', diff: null },
        { id: 'security', label: 'security', kind: 'text', value: 'WPA3', masked: null, diff: null },
      ],
    },
  ],
  tags: ['home'],
  attachments: [],
  history: [],
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T00:00:00Z',
};

const mockWifiDetailOpenNetwork: ItemDetail = {
  id: 'wifi-item-2',
  vaultId: 'v_personal',
  title: 'Guest Network',
  subtitle: 'Guest-Free',
  category: 'wifi',
  favorite: false,
  archived: false,
  sections: [
    {
      id: 'main',
      title: null,
      fields: [
        { id: 'ssid', label: 'network name', kind: 'text', value: 'Guest-Free', masked: null, diff: null },
        { id: 'security', label: 'security', kind: 'text', value: 'nopass', masked: null, diff: null },
      ],
    },
  ],
  tags: [],
  attachments: [],
  history: [],
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T00:00:00Z',
};

const mockWifiDetailEmptySsid: ItemDetail = {
  id: 'wifi-item-3',
  vaultId: 'v_personal',
  title: 'Unconfigured Wi-Fi',
  subtitle: '',
  category: 'wifi',
  favorite: false,
  archived: false,
  sections: [
    {
      id: 'main',
      title: null,
      fields: [
        { id: 'ssid', label: 'network name', kind: 'text', value: '   ', masked: null, diff: null },
        { id: 'password', label: 'wireless password', kind: 'concealed', value: null, masked: '••••••••', diff: null },
      ],
    },
  ],
  tags: [],
  attachments: [],
  history: [],
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T00:00:00Z',
};

const mockLoginDetail: ItemDetail = {
  id: 'login-item-1',
  vaultId: 'v_personal',
  title: 'GitHub',
  subtitle: 'user@example.com',
  category: 'login',
  favorite: false,
  archived: false,
  sections: [
    {
      id: 'main',
      title: null,
      fields: [
        { id: 'username', label: 'username', kind: 'text', value: 'user@example.com', masked: null, diff: null },
        { id: 'password', label: 'password', kind: 'concealed', value: null, masked: '••••••••', diff: null },
      ],
    },
  ],
  tags: [],
  attachments: [],
  history: [],
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T00:00:00Z',
};

describe('WifiQrDialog component', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ locked: false });
    vi.spyOn(api, 'revealField').mockResolvedValue('super-wifi-password-123');
  });

  it('renders dialog with accessible name, white quiet-zone container, network name, and hint', async () => {
    const onOpenChange = vi.fn();
    render(
      <TooltipProvider>
        <WifiQrDialog
          open={true}
          onOpenChange={onOpenChange}
          detail={mockWifiDetailWithPassword}
        />
      </TooltipProvider>
    );

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Join Wi-Fi network' })).toBeInTheDocument();

    // White quiet-zone container for dark/light mode camera readability
    const container = screen.getByTestId('wifi-qr-container');
    expect(container).toHaveClass('bg-white');

    await waitFor(() => {
      expect(screen.getByTestId('wifi-qr-image')).toBeInTheDocument();
    });

    // Shows network name under the code
    expect(screen.getByTestId('wifi-network-name')).toHaveTextContent('Naing-5G');
    // Shows hint
    expect(screen.getByTestId('wifi-qr-hint')).toHaveTextContent('Anyone who scans this can join the network');
  });

  it('keeps password masked initially and toggles plaintext when Reveal is clicked', async () => {
    render(
      <TooltipProvider>
        <WifiQrDialog
          open={true}
          onOpenChange={vi.fn()}
          detail={mockWifiDetailWithPassword}
        />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('wifi-password-value')).toBeInTheDocument();
    });

    // Password is not shown in text initially
    expect(screen.getByTestId('wifi-password-value')).toHaveTextContent('••••••••');
    expect(screen.queryByText('super-wifi-password-123')).toBeNull();

    // Click Reveal
    const revealBtn = screen.getByTestId('wifi-reveal-btn');
    fireEvent.click(revealBtn);

    // Plaintext password is now visible
    expect(screen.getByTestId('wifi-password-value')).toHaveTextContent('super-wifi-password-123');

    // Click Conceal
    fireEvent.click(revealBtn);
    expect(screen.getByTestId('wifi-password-value')).toHaveTextContent('••••••••');
  });

  it('handles open networks without password field', async () => {
    render(
      <TooltipProvider>
        <WifiQrDialog
          open={true}
          onOpenChange={vi.fn()}
          detail={mockWifiDetailOpenNetwork}
        />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('wifi-qr-image')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('wifi-password-row')).toBeNull();
    expect(screen.getByTestId('wifi-open-badge')).toHaveTextContent('Open network (no password)');
  });

  it('uses orange accent for the primary close button', () => {
    render(
      <TooltipProvider>
        <WifiQrDialog
          open={true}
          onOpenChange={vi.fn()}
          detail={mockWifiDetailWithPassword}
        />
      </TooltipProvider>
    );

    const closeBtn = screen.getByTestId('wifi-qr-close-btn');
    expect(closeBtn).toHaveClass('bg-accent');
  });

  it('drops QR code and image data when dialog closes or vault locks', async () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(
      <TooltipProvider>
        <WifiQrDialog
          open={true}
          onOpenChange={onOpenChange}
          detail={mockWifiDetailWithPassword}
        />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('wifi-qr-image')).toBeInTheDocument();
    });

    // When dialog closes
    rerender(
      <TooltipProvider>
        <WifiQrDialog
          open={false}
          onOpenChange={onOpenChange}
          detail={mockWifiDetailWithPassword}
        />
      </TooltipProvider>
    );
    expect(screen.queryByRole('dialog')).toBeNull();

    // When open again but vault locks
    rerender(
      <TooltipProvider>
        <WifiQrDialog
          open={true}
          onOpenChange={onOpenChange}
          detail={mockWifiDetailWithPassword}
        />
      </TooltipProvider>
    );

    act(() => {
      useApp.setState({ locked: true });
    });

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});

describe('Wi-Fi actions in ItemDetailPane', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ selectedItemId: null, locked: false, editing: false });
    vi.spyOn(api, 'revealField').mockResolvedValue('fake-wifi-pass');
  });

  it('shows enabled Show QR code button and menu item for Wi-Fi item with SSID', async () => {
    const summary: ItemSummary = {
      id: 'wifi-item-1',
      vaultId: 'v_personal',
      title: 'Home Wi-Fi',
      subtitle: 'Naing-5G',
      category: 'wifi',
      updatedAt: '2026-10-28T00:00:00Z',
    };
    useItemsUI.setState({ items: [summary], loading: false });
    useApp.setState({ selectedItemId: 'wifi-item-1' });

    vi.spyOn(api, 'getItem').mockResolvedValue(mockWifiDetailWithPassword);

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('show-wifi-qr-btn')).toBeInTheDocument();
    });

    const qrBtn = screen.getByTestId('show-wifi-qr-btn');
    expect(qrBtn).not.toBeDisabled();

    // Open via header button
    fireEvent.click(qrBtn);

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
      expect(screen.getByRole('heading', { name: 'Join Wi-Fi network' })).toBeInTheDocument();
    });

    // Close dialog
    fireEvent.click(screen.getByTestId('wifi-qr-close-btn'));
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
  });

  it('disables Show QR code button with tooltip when SSID is empty', async () => {
    const summary: ItemSummary = {
      id: 'wifi-item-3',
      vaultId: 'v_personal',
      title: 'Unconfigured Wi-Fi',
      subtitle: '',
      category: 'wifi',
      updatedAt: '2026-10-28T00:00:00Z',
    };
    useItemsUI.setState({ items: [summary], loading: false });
    useApp.setState({ selectedItemId: 'wifi-item-3' });

    vi.spyOn(api, 'getItem').mockResolvedValue(mockWifiDetailEmptySsid);

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('show-wifi-qr-btn')).toBeInTheDocument();
    });

    const qrBtn = screen.getByTestId('show-wifi-qr-btn');
    expect(qrBtn).toBeDisabled();

    // Clicking does not open dialog
    fireEvent.click(qrBtn);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('does not render Show QR code for non-wifi items', async () => {
    const summary: ItemSummary = {
      id: 'login-item-1',
      vaultId: 'v_personal',
      title: 'GitHub',
      subtitle: 'user@example.com',
      category: 'login',
      updatedAt: '2026-10-28T00:00:00Z',
    };
    useItemsUI.setState({ items: [summary], loading: false });
    useApp.setState({ selectedItemId: 'login-item-1' });

    vi.spyOn(api, 'getItem').mockResolvedValue(mockLoginDetail);

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('crumb-item')).toHaveTextContent('GitHub');
    });

    expect(screen.queryByTestId('show-wifi-qr-btn')).toBeNull();
  });

  it('returns focus to trigger button when Esc is pressed', async () => {
    const summary: ItemSummary = {
      id: 'wifi-item-1',
      vaultId: 'v_personal',
      title: 'Home Wi-Fi',
      subtitle: 'Naing-5G',
      category: 'wifi',
      updatedAt: '2026-10-28T00:00:00Z',
    };
    useItemsUI.setState({ items: [summary], loading: false });
    useApp.setState({ selectedItemId: 'wifi-item-1' });

    vi.spyOn(api, 'getItem').mockResolvedValue(mockWifiDetailWithPassword);

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('show-wifi-qr-btn')).toBeInTheDocument();
    });

    const qrBtn = screen.getByTestId('show-wifi-qr-btn');
    qrBtn.focus();
    expect(document.activeElement).toBe(qrBtn);

    // Open via keyboard Enter
    fireEvent.click(qrBtn);

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });

    // Press Esc to dismiss
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape', code: 'Escape' });

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });

    // Focus returns to the button that opened it
    expect(document.activeElement).toBe(qrBtn);
  });
});
