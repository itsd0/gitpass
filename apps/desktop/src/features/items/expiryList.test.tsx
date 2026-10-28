import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock @tanstack/react-virtual so jsdom renders all rows
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0,
    scrollToIndex: () => {},
    getVirtualItems: () =>
      Array.from({ length: o.count }, (_, i) => ({
        index: i,
        start: 0,
        size: o.estimateSize(i),
        key: i,
      })),
  }),
}));

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ItemList } from "./ItemList";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import type { ItemSummary } from "../../api/types";
import { api } from "../../api/client";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ItemList expiry pills", () => {
  const fixedNow = new Date("2026-05-15T12:00:00.000Z");

  const mockItems: ItemSummary[] = [
    {
      id: "card_expired",
      vaultId: "v1",
      category: "credit_card",
      title: "Expired Travel Visa",
      subtitle: "•••• 1111",
      favorite: false,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: "2026-05-01T00:00:00Z",
      createdAt: "2024-01-01T00:00:00Z",
      expiry: "03/2026", // Expired end of March 2026
    },
    {
      id: "api_expiring",
      vaultId: "v1",
      category: "api_credential",
      title: "Production OpenAI Token",
      subtitle: "svc-prod",
      favorite: true,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: "2026-05-10T00:00:00Z",
      createdAt: "2025-05-10T00:00:00Z",
      expiry: "2026-05-27", // Expires in 12 days
    },
    {
      id: "login_normal",
      vaultId: "v1",
      category: "login",
      title: "Normal GitHub Login",
      subtitle: "octocat@example.invalid",
      favorite: false,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: "2026-05-12T00:00:00Z",
      createdAt: "2024-05-12T00:00:00Z",
    },
    {
      id: "card_valid",
      vaultId: "v1",
      category: "credit_card",
      title: "Valid Business Mastercard",
      subtitle: "•••• 4444",
      favorite: false,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: "2026-05-10T00:00:00Z",
      createdAt: "2025-01-01T00:00:00Z",
      expiry: "12/2030", // Long valid
    },
  ];

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(fixedNow);

    useApp.setState({
      route: { name: "items", filter: { kind: "all" } },
      selectedItemId: null,
      multiIds: [],
    });

    vi.spyOn(api, "listItems").mockResolvedValue(mockItems);
    useItemsUI.setState({ items: mockItems, loading: false });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders expired pill on expired card, expires soon on expiring credential, and nothing on normal login", async () => {
    render(<ItemList />);

    // Check Expired pill
    const expiredPill = screen.getByLabelText("Expired");
    expect(expiredPill).toBeInTheDocument();
    expect(expiredPill.textContent).toBe("Expired");
    expect(expiredPill.className).toContain("text-danger");

    // Check Expires soon pill
    const soonPill = screen.getByLabelText("Expires soon");
    expect(soonPill).toBeInTheDocument();
    expect(soonPill.textContent).toBe("Expires soon");
    expect(soonPill.className).toContain("text-warn");

    // Total count of expiry pills across all rows must be exactly 2
    const allPills = screen.getAllByTestId("expiry-pill");
    expect(allPills).toHaveLength(2);

    // Assert that the pills are within the appropriate rows
    const expiredRow = screen.getByText("Expired Travel Visa").closest("[role='option']");
    expect(expiredRow).toContainElement(expiredPill);

    const soonRow = screen.getByText("Production OpenAI Token").closest("[role='option']");
    expect(soonRow).toContainElement(soonPill);

    const normalRow = screen.getByText("Normal GitHub Login").closest("[role='option']");
    expect(normalRow?.querySelector("[data-testid='expiry-pill']")).toBeNull();

    const validCardRow = screen.getByText("Valid Business Mastercard").closest("[role='option']");
    expect(validCardRow?.querySelector("[data-testid='expiry-pill']")).toBeNull();
  });

  it("does not change list keyboard navigation when pills are present", () => {
    render(<ItemList />);

    const listbox = screen.getByRole("listbox", { name: "Items" });
    listbox.focus();

    // Auto-selects first item
    expect(useApp.getState().selectedItemId).toBe("card_expired");

    // Arrow down moves to next item
    fireEvent.keyDown(listbox, { key: "ArrowDown" });
    expect(useApp.getState().selectedItemId).toBe("api_expiring");

    // Arrow down moves to normal login
    fireEvent.keyDown(listbox, { key: "ArrowDown" });
    expect(useApp.getState().selectedItemId).toBe("login_normal");

    // Arrow up moves back
    fireEvent.keyDown(listbox, { key: "ArrowUp" });
    expect(useApp.getState().selectedItemId).toBe("api_expiring");
  });
});
