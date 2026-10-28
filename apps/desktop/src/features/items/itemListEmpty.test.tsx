import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ItemList } from "./ItemList";
import { ItemListEmptyState } from "./ItemListEmptyState";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { useI18n } from "../../i18n";
import { api } from "../../api/client";

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

describe("ItemList empty states", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.getState().setLang("en");
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
    });
    vi.spyOn(api, "listItems").mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    useI18n.getState().setLang("en");
    vi.restoreAllMocks();
  });

  it("renders empty vault view with New item and Import actions", async () => {
    render(<ItemList />);
    const statusContainer = await screen.findByRole("status");
    expect(statusContainer).toBeTruthy();
    expect(screen.getByTestId("empty-title").textContent).toBe("Your vault is empty");
    expect(screen.getByTestId("empty-subtitle").textContent).toBe(
      "Add your first password, login, or card to get started."
    );

    const newItemBtn = screen.getByRole("button", { name: "New item" });
    const importBtn = screen.getByRole("button", { name: "Import" });
    expect(newItemBtn).toBeTruthy();
    expect(importBtn).toBeTruthy();

    fireEvent.click(newItemBtn);
    expect(useApp.getState().modal).toEqual({ id: "new-item", props: undefined });

    fireEvent.click(importBtn);
    expect(useApp.getState().modal).toEqual({ id: "new-item", props: { view: "import" } });
  });

  it("renders search with no results view, allows clearing search and creating item", async () => {
    useItemsUI.setState({ query: "github login" });
    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "No results for “github login”"
    );
    expect(screen.getByTestId("empty-subtitle").textContent).toBe(
      "Check the spelling or try a different search."
    );

    const createBtn = screen.getByRole("button", { name: "Create “github login”" });
    const clearBtn = screen.getByRole("button", { name: "Clear search" });
    expect(createBtn).toBeTruthy();
    expect(clearBtn).toBeTruthy();

    // Clicking Create “q” opens New item -> Login with title prefilled
    fireEvent.click(createBtn);
    expect(useApp.getState().modal).toEqual({
      id: "new-item",
      props: {
        category: "login",
        initial: { title: "github login" },
      },
    });

    // Clicking Clear search empties the query in useItemsUI
    fireEvent.click(clearBtn);
    expect(useItemsUI.getState().query).toBe("");
  });

  it("renders category filter empty state with New <category> action", async () => {
    useItemsUI.setState({ category: "login" });
    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe("No Login items");
    expect(screen.getByTestId("empty-subtitle").textContent).toBe(
      "Add a Login to your vault to keep it secure."
    );

    const newCatBtn = screen.getByRole("button", { name: "New Login" });
    expect(newCatBtn).toBeTruthy();
    fireEvent.click(newCatBtn);
    expect(useApp.getState().modal).toEqual({
      id: "new-item",
      props: { category: "login" },
    });

    // Also test credit_card category
    useItemsUI.setState({ category: "credit_card" });
    cleanup();
    render(<ItemList />);
    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "No Credit Card items"
    );
    expect(screen.getByRole("button", { name: "New Credit Card" })).toBeTruthy();
  });

  it("renders favorites view with star hint and no buttons", async () => {
    useApp.setState({
      route: { name: "items", filter: { kind: "favorites" } },
    });
    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe("No favorites yet");
    expect(screen.getByTestId("empty-subtitle").textContent).toContain("star button");
    expect(screen.queryByRole("button", { name: /New/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Import/ })).toBeNull();
  });

  it("renders archive view with explanation and no buttons", async () => {
    useApp.setState({
      route: { name: "items", filter: { kind: "archive" } },
    });
    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe("Nothing archived");
    expect(screen.getByTestId("empty-subtitle").textContent).toContain("Archived items");
    expect(screen.queryByRole("button", { name: /New/ })).toBeNull();
  });

  it("renders recently deleted view with 30-day purge note and no buttons", async () => {
    useApp.setState({
      route: { name: "items", filter: { kind: "deleted" } },
    });
    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "Recently Deleted is empty"
    );
    expect(screen.getByTestId("empty-subtitle").textContent).toContain("30 days");
    expect(screen.queryByRole("button", { name: /New/ })).toBeNull();
  });

  it("renders tag view with tag name and no buttons", async () => {
    useApp.setState({
      route: { name: "items", filter: { kind: "tag", tag: "servers" } },
    });
    render(<ItemList />);

    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "No items tagged #servers"
    );
    expect(screen.getByTestId("empty-subtitle").textContent).toContain("Add this tag");
    expect(screen.queryByRole("button", { name: /New/ })).toBeNull();
  });

  it("renders localized Burmese strings across views", async () => {
    useI18n.getState().setLang("my");

    // 1. Empty vault
    render(<ItemList />);
    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "သင့် သော့ခတ်သေတ္တာထဲတွင် ဘာမှမရှိသေးပါ"
    );
    expect(screen.getByRole("button", { name: "အရာအသစ်" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "တင်သွင်းရန်" })).toBeTruthy();

    // 2. Search
    useItemsUI.setState({ query: "server" });
    cleanup();
    render(<ItemList />);
    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "“server” အတွက် ရလဒ် မရှိပါ"
    );
    expect(
      screen.getByRole("button", { name: "“server” အသစ် ဖန်တီးရန်" })
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "ရှာဖွေမှု ရှင်းရန်" })).toBeTruthy();

    // 3. Category
    useItemsUI.setState({ query: "", category: "login" });
    cleanup();
    render(<ItemList />);
    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "လော့ဂ်အင် မရှိသေးပါ"
    );
    expect(screen.getByRole("button", { name: "လော့ဂ်အင် အသစ်" })).toBeTruthy();

    // 4. Favorites
    useItemsUI.setState({ category: null });
    useApp.setState({
      route: { name: "items", filter: { kind: "favorites" } },
    });
    cleanup();
    render(<ItemList />);
    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "အကြိုက်ဆုံးများ မရှိသေးပါ"
    );

    // 5. Tag
    useApp.setState({
      route: { name: "items", filter: { kind: "tag", tag: "work" } },
    });
    cleanup();
    render(<ItemList />);
    expect((await screen.findByTestId("empty-title")).textContent).toBe(
      "#work တဂ်တပ်ထားသော အရာ မရှိပါ"
    );
  });

  it("shows loading skeleton while loading and hides empty state", () => {
    useItemsUI.setState({ items: [], loading: true });
    render(<ItemList />);
    expect(screen.queryByTestId("item-list-empty")).toBeNull();
    // Skeleton elements are rendered
    const skeletons = document.querySelectorAll(".animate-pulse");
    expect(skeletons.length).toBeGreaterThan(0);
  });

  it("can also render ItemListEmptyState directly with custom props", () => {
    render(
      <ItemListEmptyState
        query="test"
        onClearSearch={vi.fn()}
        onCreateItem={vi.fn()}
      />
    );
    expect(screen.getByTestId("empty-title").textContent).toBe("No results for “test”");
  });
});
