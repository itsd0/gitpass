import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock @tanstack/react-virtual so jsdom renders every row.
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
import { useI18n } from "../../i18n";
import { api } from "../../api/client";
import type { ItemSummary } from "../../api/types";

function summary(over: Partial<ItemSummary>): ItemSummary {
  return {
    id: "item",
    vaultId: "v-test",
    category: "login",
    title: "Untitled",
    subtitle: "",
    favorite: false,
    archived: false,
    deletedAt: null,
    tags: [],
    updatedAt: "2026-01-01T00:00:00Z",
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

const githubLogin = summary({
  id: "i-github",
  title: "GitHub Login",
  subtitle: "octocat@example.invalid",
});
const workEmail = summary({
  id: "i-email",
  title: "Work Email",
  subtitle: "gitlab@example.invalid",
});

describe("ItemList search highlighting", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.getState().setLang("en");
    useApp.setState({
      route: { name: "items", filter: { kind: "all" } },
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
    useI18n.getState().setLang("en");
    vi.restoreAllMocks();
  });

  it("highlights matches in the title and in the username / domain subtitle", async () => {
    useItemsUI.setState({ query: "git" });
    vi.spyOn(api, "listItems").mockResolvedValue([githubLogin, workEmail]);

    render(<ItemList />);

    const marks = await screen.findAllByTestId("search-highlight");
    expect(marks.map((m) => m.textContent)).toEqual(["Git", "git"]);
    expect(marks[0].tagName).toBe("MARK");
    expect(marks[0].className).toContain("bg-accent/20");

    // Screen readers still get the full, unsplit title and subtitle.
    const rows = screen.getAllByRole("option");
    expect(rows[0].textContent).toContain("GitHub Login");
    expect(rows[0].textContent).toContain("octocat@example.invalid");
    expect(rows[1].textContent).toContain("gitlab@example.invalid");
  });

  it("matches case- and diacritic-insensitively in rows", async () => {
    useItemsUI.setState({ query: "resume" });
    vi.spyOn(api, "listItems").mockResolvedValue([
      summary({ id: "i-resume", title: "Résumé Builder" }),
    ]);

    render(<ItemList />);

    const marks = await screen.findAllByTestId("search-highlight");
    expect(marks.map((m) => m.textContent)).toEqual(["Résumé"]);
  });

  it("shows a localized singular and plural result count", async () => {
    const listItems = vi.spyOn(api, "listItems").mockResolvedValue([githubLogin]);
    useItemsUI.setState({ query: "git" });

    render(<ItemList />);

    expect(await screen.findByTestId("search-result-count")).toHaveTextContent(
      "1 result"
    );

    listItems.mockResolvedValue([githubLogin, workEmail]);
    cleanup();
    useItemsUI.setState({ items: [], query: "gitl" });
    render(<ItemList />);

    expect(await screen.findByTestId("search-result-count")).toHaveTextContent(
      "2 results"
    );
  });

  it("localizes the result count in Burmese and Thai", async () => {
    vi.spyOn(api, "listItems").mockResolvedValue([githubLogin, workEmail]);
    useItemsUI.setState({ query: "git" });

    useI18n.getState().setLang("my");
    render(<ItemList />);
    expect(await screen.findByTestId("search-result-count")).toHaveTextContent(
      "ရလဒ် 2 ခု"
    );

    cleanup();
    useI18n.getState().setLang("th");
    render(<ItemList />);
    expect(await screen.findByTestId("search-result-count")).toHaveTextContent(
      "ผลลัพธ์ 2 รายการ"
    );
  });

  it("renders no marks and no count line without a query", async () => {
    vi.spyOn(api, "listItems").mockResolvedValue([githubLogin]);

    render(<ItemList />);

    expect(await screen.findByText("GitHub Login")).toBeInTheDocument();
    expect(screen.queryAllByTestId("search-highlight")).toHaveLength(0);
    expect(screen.queryByTestId("search-result-count")).toBeNull();
  });

  it("keeps the localized no-results empty state for a query with no matches", async () => {
    useItemsUI.setState({ query: "zzz" });
    vi.spyOn(api, "listItems").mockResolvedValue([]);

    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "No results for “zzz”"
    );
    expect(screen.queryByTestId("search-result-count")).toBeNull();
  });

  it("keeps arrow-key selection working while results are highlighted", async () => {
    useItemsUI.setState({ query: "git" });
    vi.spyOn(api, "listItems").mockResolvedValue([githubLogin, workEmail]);

    render(<ItemList />);
    await screen.findAllByTestId("search-highlight");

    const listbox = screen.getByRole("listbox", { name: "Items" });
    listbox.focus();
    expect(useApp.getState().selectedItemId).toBe("i-github");

    fireEvent.keyDown(listbox, { key: "ArrowDown" });
    expect(useApp.getState().selectedItemId).toBe("i-email");
  });
});
