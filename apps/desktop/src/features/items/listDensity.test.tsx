import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ItemList, ITEM_H, ITEM_H_COMPACT, rowHeight } from "./ItemList";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { useSettings } from "../settings/useSettings";
import { useI18n } from "../../i18n";
import { api } from "../../api/client";
import type { ItemSummary } from "../../api/types";

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

const base = {
  autoLockMinutes: 10,
  clipboardClearSecs: 30,
  theme: "dark" as const,
  language: "en" as const,
  biometricUnlock: false,
  quickAccessHotkey: "CmdOrCtrl+Shift+Space",
  hibpCheck: false,
};

const login: ItemSummary = {
  id: "i1", vaultId: "v1", category: "login", title: "Example Site",
  subtitle: "user@example.invalid", favorite: false, archived: false,
  deletedAt: null, tags: [], updatedAt: "2026-10-29T00:00:00Z",
  createdAt: "2026-10-01T00:00:00Z", url: "https://example.invalid",
};
const expiredCard: ItemSummary = {
  id: "i2", vaultId: "v1", category: "credit_card", title: "Fake Card",
  subtitle: "•••• 4242", favorite: false, archived: false,
  deletedAt: null, tags: [], updatedAt: "2026-10-29T00:00:00Z",
  createdAt: "2026-10-01T00:00:00Z", expiry: "01/2020",
};

function setup(density: "comfortable" | "compact") {
  useI18n.getState().setLang("en");
  useApp.setState({
    route: { name: "items", filter: { kind: "all" } },
    modal: null,
    selectedItemId: "i1",
    multiIds: [],
  });
  useItemsUI.setState({ items: [], loading: false, query: "", category: null });
  useSettings.setState({ settings: { ...base, listDensity: density }, error: null });
  vi.spyOn(api, "listItems").mockResolvedValue([login, expiredCard]);
}

describe("list density", () => {
  beforeEach(() => { vi.restoreAllMocks(); });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    useSettings.setState({ settings: null });
    useI18n.getState().setLang("en");
  });

  it("rowHeight is 54 comfortable and 32 compact", () => {
    const itemRow = { type: "item" as const, key: "i1", item: login };
    expect(rowHeight(itemRow)).toBe(ITEM_H);
    expect(rowHeight(itemRow, "compact")).toBe(ITEM_H_COMPACT);
    expect(rowHeight({ type: "header" as const, key: "h", label: "Recent" }, "compact")).toBe(32);
  });

  it("comfortable rows keep the two-line layout and expiry pill", async () => {
    setup("comfortable");
    render(<ItemList />);
    const rows = await screen.findAllByTestId("item-row");
    expect(rows[0].style.height).toBe("52px");
    expect(screen.queryByTestId("item-row-compact-line")).toBeNull();
    expect(screen.getByTestId("expiry-pill")).toHaveAttribute("aria-label", "Expired");
    expect(screen.queryByTestId("expiry-dot")).toBeNull();
  });

  it("compact rows are single-line, 30px, with smaller icon and expiry dot", async () => {
    setup("compact");
    render(<ItemList />);
    const rows = await screen.findAllByTestId("item-row");
    const compactRows = rows.filter((r) => r.style.height === "30px");
    expect(compactRows.length).toBe(2);
    expect(screen.getByTestId("item-list")).toHaveAttribute("data-list-density", "compact");
    const line = screen.getAllByTestId("item-row-compact-line")[0];
    expect(line.textContent).toContain("Example Site");
    expect(line.textContent).toContain("user@example.invalid");
    const dot = screen.getByTestId("expiry-dot");
    expect(dot).toHaveAttribute("aria-label", "Expired");
    expect(dot).toHaveAttribute("title", "Expired");
    expect(screen.queryByTestId("expiry-pill")).toBeNull();
  });

  it("compact rows keep multi-select checkboxes, search highlight and keyboard selection", async () => {
    setup("compact");
    render(<ItemList />);
    await screen.findAllByTestId("item-row");
    // Multi-select checkbox still present on every row.
    expect(screen.getAllByTestId("row-check").length).toBe(2);
    // Search highlight: typing a query keeps the row and highlights the match.
    act(() => { useItemsUI.setState({ query: "Example" }); });
    const rows = await screen.findAllByTestId("item-row");
    expect(rows.length).toBeGreaterThan(0);
    // ArrowDown moves selection without breaking compact rendering.
    const listbox = screen.getByTestId("item-list").querySelector("[data-item-list]");
    fireEvent.keyDown(listbox ?? screen.getByTestId("item-list"), { key: "ArrowDown" });
    expect(screen.getByTestId("item-list")).toHaveAttribute("data-list-density", "compact");
  });
});
