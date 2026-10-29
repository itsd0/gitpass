import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ItemList } from "./ItemList";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { api } from "../../api/client";
import type { ItemSummary } from "../../api/types";
import { useI18n } from "../../i18n";

vi.mock("@tanstack/react-virtual", () => ({
  defaultRangeExtractor: (range: { startIndex: number; endIndex: number }) => {
    const list: number[] = [];
    for (let i = range.startIndex; i <= range.endIndex; i++) list.push(i);
    return list;
  },
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

const makeItem = (
  id: string,
  title: string,
  updatedAt: string,
  createdAt: string = updatedAt,
  lastUsed?: number
): ItemSummary => ({
  id,
  vaultId: "v1",
  category: "login",
  title,
  subtitle: "user@example.com",
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt,
  createdAt,
  lastUsed,
});

describe("Desktop ItemList date section headers", () => {
  const now = new Date();
  const todayIso = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 10, 0, 0).toISOString();
  const yesterdayIso = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1, 10, 0, 0).toISOString();
  const prev7Iso = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 4, 10, 0, 0).toISOString();
  const prev30Iso = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 15, 10, 0, 0).toISOString();
  const olderIso = new Date(2025, 5, 15, 10, 0, 0).toISOString(); // June 2025

  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.getState().setLang("en");
    useApp.setState({
      route: { name: "items", filter: { kind: "vault", vaultId: "v1" } }, // using vault view so showRecentSection is false
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

  it("renders date section headers when sorted by Date Edited (updated)", async () => {
    const items = [
      makeItem("1", "Apple Login", todayIso),
      makeItem("2", "Banana Account", yesterdayIso),
      makeItem("3", "Cherry Server", prev7Iso),
      makeItem("4", "Date Database", prev30Iso),
      makeItem("5", "Elderberry Key", olderIso),
    ];
    useItemsUI.setState({ items, loading: false, sort: "updated" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    const headerLabels = headers.map((h) => h.textContent?.trim());

    expect(headerLabels).toContain("Today");
    expect(headerLabels).toContain("Yesterday");
    expect(headerLabels).toContain("Previous 7 days");
    expect(headerLabels).toContain("Previous 30 days");
    expect(headerLabels).toContain("June 2025");
  });

  it("renders date section headers when sorted by Date Created (created)", async () => {
    const items = [
      makeItem("1", "First Service", todayIso, todayIso),
      makeItem("2", "Second Service", yesterdayIso, yesterdayIso),
    ];
    useItemsUI.setState({ items, loading: false, sort: "created" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    const headerLabels = headers.map((h) => h.textContent?.trim());

    expect(headerLabels).toEqual(["Today", "Yesterday"]);
  });

  it("renders date headers and Never used when sorted by Recently used (recent)", async () => {
    const nowSecs = Math.floor(Date.now() / 1000);
    const yesterdaySecs = nowSecs - 86400;
    const items = [
      makeItem("1", "Active Today", todayIso, todayIso, nowSecs),
      makeItem("2", "Active Yesterday", yesterdayIso, yesterdayIso, yesterdaySecs),
      makeItem("3", "Never Touched", todayIso, todayIso, undefined),
    ];
    useItemsUI.setState({ items, loading: false, sort: "recent" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    const headerLabels = headers.map((h) => h.textContent?.trim());

    expect(headerLabels).toContain("Today");
    expect(headerLabels).toContain("Yesterday");
    expect(headerLabels).toContain("Never used");
  });

  it("renders letter headers and NO date headers when sorted by Title", async () => {
    const items = [
      makeItem("1", "Alpha", todayIso),
      makeItem("2", "Beta", yesterdayIso),
      makeItem("3", "Delta", prev7Iso),
    ];
    useItemsUI.setState({ items, loading: false, sort: "title" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    const headerLabels = headers.map((h) => h.textContent?.trim());

    // Letter headers only
    expect(headerLabels).toEqual(["A", "B", "D"]);

    // Must not contain any date headers
    expect(headerLabels).not.toContain("Today");
    expect(headerLabels).not.toContain("Yesterday");
    expect(headerLabels).not.toContain("Previous 7 days");
    expect(headerLabels).not.toContain("Previous 30 days");
  });

  it("provides accessibility attributes on headers and items", async () => {
    const items = [
      makeItem("1", "Alpha", todayIso),
      makeItem("2", "Beta", yesterdayIso),
    ];
    useItemsUI.setState({ items, loading: false, sort: "updated" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    expect(headers[0]).toHaveAttribute("role", "group");
    expect(headers[0]).toHaveAttribute("aria-label", "Today");
    expect(headers[0]).toHaveAttribute("id", "header-h:today");

    const optionRows = screen.getAllByRole("option");
    expect(optionRows[0]).toHaveAttribute("aria-describedby", "header-h:today");
    expect(optionRows[1]).toHaveAttribute("aria-describedby", "header-h:yesterday");
  });

  it("skips headers during keyboard arrow navigation", async () => {
    const items = [
      makeItem("1", "Alpha", todayIso),
      makeItem("2", "Beta", yesterdayIso),
    ];
    useItemsUI.setState({ items, loading: false, sort: "updated" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    render(<ItemList />);

    expect(useApp.getState().selectedItemId).toBe("1");

    const listbox = screen.getByRole("listbox");
    act(() => {
      fireEvent.keyDown(listbox, { key: "ArrowDown" });
    });

    // Arrow down moves directly from item 1 to item 2, skipping the "Yesterday" header
    expect(useApp.getState().selectedItemId).toBe("2");

    act(() => {
      fireEvent.keyDown(listbox, { key: "ArrowUp" });
    });
    expect(useApp.getState().selectedItemId).toBe("1");
  });

  it("hides empty groups when search filtering is active", async () => {
    const items = [
      makeItem("1", "GitHub Login", todayIso),
      makeItem("2", "Google Cloud", yesterdayIso),
    ];
    // Filtering by "GitHub": only item 1 matches
    useItemsUI.setState({ items: [items[0]], loading: false, sort: "updated", query: "GitHub" });
    vi.spyOn(api, "listItems").mockResolvedValue([items[0]]);

    render(<ItemList />);

    const headers = screen.getAllByTestId("group-header");
    const headerLabels = headers.map((h) => h.textContent?.trim());

    expect(headerLabels).toEqual(["Today"]);
    expect(headerLabels).not.toContain("Yesterday");
  });

  it("localizes section headers in Burmese (my) and Thai (th)", async () => {
    const items = [
      makeItem("1", "Alpha", todayIso),
      makeItem("2", "Beta", yesterdayIso),
    ];
    useItemsUI.setState({ items, loading: false, sort: "updated" });
    vi.spyOn(api, "listItems").mockResolvedValue(items);

    // Switch to Thai
    useI18n.getState().setLang("th");
    const { unmount } = render(<ItemList />);

    let headers = screen.getAllByTestId("group-header");
    expect(headers[0].textContent?.trim()).toBe("วันนี้");
    expect(headers[1].textContent?.trim()).toBe("เมื่อวานนี้");

    unmount();

    // Switch to Burmese
    useI18n.getState().setLang("my");
    render(<ItemList />);

    headers = screen.getAllByTestId("group-header");
    expect(headers[0].textContent?.trim()).toBe("ယနေ့");
    expect(headers[1].textContent?.trim()).toBe("မနေ့က");
  });
});
