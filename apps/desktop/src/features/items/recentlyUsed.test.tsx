import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ItemList } from "./ItemList";
import { ItemDetailPane } from "./ItemDetail";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { api } from "../../api/client";
import type { ItemSummary, ItemDetail as DetailType } from "../../api/types";
import { TooltipProvider } from "../../ui";

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

const mockSummary = (id: string, title: string, lastUsed?: number): ItemSummary => ({
  id,
  vaultId: "v1",
  category: "login",
  title,
  subtitle: "user@example.com",
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: "2026-10-20T00:00:00Z",
  createdAt: "2026-10-01T00:00:00Z",
  lastUsed,
});

describe("Recently used list and detail", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({
      route: { name: "items", filter: { kind: "all" } },
      modal: null,
      selectedItemId: null,
      multiIds: [],
    });
    useItemsUI.setState({
      items: [],
      loading: false,
      query: "",
      category: null,
      sort: "updated",
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders a Recently used section at the top of All Items view with keyboard navigation", async () => {
    const items = [
      mockSummary("i1", "GitHub", 1793210000), // recent
      mockSummary("i2", "Google", 1793200000), // recent
      mockSummary("i3", "Slack"),              // never used
    ];
    useItemsUI.setState({ items, loading: false });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    expect(headers[0].textContent).toBe("Recently used");

    const options = screen.getAllByRole("option");
    expect(options.length).toBe(3);

    // Initial selected is i1 (first item in list)
    expect(useApp.getState().selectedItemId).toBe("i1");

    // Arrow down moves through the list: i1 -> i2 -> i3
    const listbox = screen.getByRole("listbox");
    act(() => {
      fireEvent.keyDown(listbox, { key: "ArrowDown" });
    });
    expect(useApp.getState().selectedItemId).toBe("i2");

    act(() => {
      fireEvent.keyDown(listbox, { key: "ArrowDown" });
    });
    expect(useApp.getState().selectedItemId).toBe("i3");
  });

  it("supports sorting by recently used", async () => {
    const items = [
      mockSummary("i1", "Beta", Math.floor(Date.now() / 1000)),
      mockSummary("i2", "Alpha"),
    ];
    useItemsUI.setState({ items, loading: false, sort: "recent" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    expect(headers[0].textContent).toBe("Today");
    expect(headers[1].textContent).toBe("Never used");
  });

  it("displays 'Last used: 3 days ago' in ItemDetail header", async () => {
    const threeDaysAgoSecs = Math.floor((Date.now() - 3 * 86400 * 1000 - 3600 * 1000) / 1000);
    const summary = mockSummary("i1", "GitHub", threeDaysAgoSecs);
    const detail: DetailType = {
      ...summary,
      sections: [],
      notes: null,
      attachments: [],
      history: [],
      hasTotp: false,
    };

    useItemsUI.setState({ items: [summary], loading: false });
    useApp.setState({ selectedItemId: "i1" });
    vi.spyOn(api, "getItem").mockResolvedValue(detail);
    vi.spyOn(api, "listVaults").mockResolvedValue([{ id: "v1", name: "Personal", description: null, itemCount: 1, shared: false, members: [] }]);

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    const hint = await screen.findByTestId("last-used");
    expect(hint.textContent).toBe("Last used: 3 days ago");
  });

  it("does not render last-used hint when item was never used", async () => {
    const summary = mockSummary("i1", "GitHub");
    const detail: DetailType = {
      ...summary,
      sections: [],
      notes: null,
      attachments: [],
      history: [],
      hasTotp: false,
    };

    useItemsUI.setState({ items: [summary], loading: false });
    useApp.setState({ selectedItemId: "i1" });
    vi.spyOn(api, "getItem").mockResolvedValue(detail);
    vi.spyOn(api, "listVaults").mockResolvedValue([{ id: "v1", name: "Personal", description: null, itemCount: 1, shared: false, members: [] }]);

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await screen.findByRole("heading", { name: "GitHub" });
    expect(screen.queryByTestId("last-used")).toBeNull();
  });
});
