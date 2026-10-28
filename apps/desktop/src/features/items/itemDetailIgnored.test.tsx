import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import { ItemDetailPane } from "./ItemDetail";
import { TooltipProvider } from "../../ui";
import { api } from "../../api/client";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import type { ItemDetail as Detail, ItemSummary, VaultInfo } from "../../api/types";

const mockVault: VaultInfo = {
  id: "v-1",
  name: "Personal",
  description: "Personal vault",
  itemCount: 1,
  shared: false,
  members: [],
};

const mockItemWithIgnored: Detail = {
  id: "item-1",
  vaultId: "v-1",
  category: "login",
  title: "Example Website",
  subtitle: "user@example.com",
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: "2026-10-28T00:00:00Z",
  createdAt: "2026-10-20T00:00:00Z",
  url: "http://example.com",
  sections: [
    {
      id: "main",
      title: null,
      fields: [
        { id: "username", label: "username", kind: "text", value: "user@example.com" },
        { id: "password", label: "password", kind: "concealed", value: null },
      ],
    },
  ],
  notes: null,
  attachments: [],
  history: [],
  passwordStrength: 30,
  hasTotp: false,
  ignoredFindings: ["reused", "insecure_url"],
};

const mockItemClean: Detail = {
  ...mockItemWithIgnored,
  id: "item-2",
  title: "Clean Item",
  ignoredFindings: [],
};

describe("ItemDetail Watchtower Ignored Note", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listVaults").mockResolvedValue([mockVault]);
    vi.spyOn(api, "listTags").mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  it("shows muted note and undo link when item has ignored findings", async () => {
    const unignoreSpy = vi.spyOn(api, "unignoreFinding").mockResolvedValue();
    vi.spyOn(api, "getItem").mockResolvedValue(mockItemWithIgnored);
    vi.spyOn(api, "listItems").mockResolvedValue([mockItemWithIgnored as ItemSummary]);

    useItemsUI.setState({ items: [mockItemWithIgnored as any], loading: false });
    useApp.setState({ selectedItemId: "item-1", editing: false });

    render(<TooltipProvider><ItemDetailPane /></TooltipProvider>);

    const note = await screen.findByTestId("watchtower-ignored-note");
    expect(note).toBeInTheDocument();
    expect(note.textContent).toContain("Watchtower warning ignored");

    const undoBtn = screen.getByRole("button", { name: /undo/i });
    expect(undoBtn).toBeInTheDocument();

    fireEvent.click(undoBtn);

    await waitFor(() => {
      expect(unignoreSpy).toHaveBeenCalledWith({ vaultId: "v-1", itemId: "item-1", kind: "reused" });
      expect(unignoreSpy).toHaveBeenCalledWith({ vaultId: "v-1", itemId: "item-1", kind: "insecure_url" });
    });
  });

  it("does not render muted note when item has no ignored findings", async () => {
    vi.spyOn(api, "getItem").mockResolvedValue(mockItemClean);
    vi.spyOn(api, "listItems").mockResolvedValue([mockItemClean as ItemSummary]);

    useItemsUI.setState({ items: [mockItemClean as any], loading: false });
    useApp.setState({ selectedItemId: "item-2", editing: false });

    render(<TooltipProvider><ItemDetailPane /></TooltipProvider>);

    await screen.findAllByText("Clean Item");
    expect(screen.queryByTestId("watchtower-ignored-note")).toBeNull();
  });
});
