import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ItemDetailPane } from "./ItemDetail";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { api } from "../../api/client";
import { Toaster, TooltipProvider, useToasts } from "../../ui";
import type { ItemDetail, ItemSummary, VaultInfo } from "../../api/types";

const mockVault: VaultInfo = {
  id: "v1",
  name: "Personal",
  description: "Personal items",
  itemCount: 1,
  shared: false,
  members: [],
  icon: "vault",
  color: "#f97316",
};

const mockSummary: ItemSummary = {
  id: "item-1",
  vaultId: "v1",
  category: "login",
  title: "GitHub (v2)",
  subtitle: "alice@example.com",
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: ["dev"],
  updatedAt: "2026-10-28T00:00:00Z",
  createdAt: "2026-10-20T00:00:00Z",
};

const mockDetail: ItemDetail = {
  ...mockSummary,
  version: 2,
  sections: [
    {
      id: "sec-1",
      title: "Login credentials",
      fields: [
        { id: "f1", kind: "text", label: "username", value: "alice@example.com" },
        { id: "f2", kind: "concealed", label: "password", value: null },
        { id: "f3", kind: "text", label: "note", value: "new note" },
      ],
    },
  ],
  notes: "My notes here",
  attachments: [],
  history: [
    { version: 1, updatedAt: "2026-10-20T00:00:00Z" },
    { version: 2, updatedAt: "2026-10-28T00:00:00Z" },
  ],
  hasTotp: false,
};

const mockVersion1Snapshot: ItemDetail = {
  ...mockSummary,
  title: "GitHub (v1)",
  version: 1,
  sections: [
    {
      id: "sec-1",
      title: "Login credentials",
      fields: [
        { id: "f1", kind: "text", label: "username", value: "alice@example.com", diff: null },
        { id: "f2", kind: "concealed", label: "password", value: null, diff: "changed" },
        { id: "f3", kind: "text", label: "note", value: "new note", diff: "added" },
        { id: "f0", kind: "text", label: "legacy", value: "old info", diff: "removed" },
      ],
    },
  ],
  notes: "Old notes",
  attachments: [],
  history: [],
  hasTotp: false,
};

describe("History and restore flow", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useToasts.setState({ list: [] });
    useItemsUI.setState({ items: [mockSummary], loading: false });
    useApp.setState({
      selectedItemId: "item-1",
      editing: false,
      itemsVersion: 1,
      locked: false,
      route: { name: "items", filter: { kind: "all" } },
    });

    vi.spyOn(api, "getItem").mockResolvedValue(mockDetail);
    vi.spyOn(api, "listVaults").mockResolvedValue([mockVault]);
    vi.spyOn(api, "itemVersion").mockResolvedValue(mockVersion1Snapshot);
    vi.spyOn(api, "revealField").mockResolvedValue("FAKE-secret-revealed-pw");
    vi.spyOn(api, "restoreItemVersion").mockResolvedValue({
      ...mockDetail,
      title: "GitHub (v1)",
      version: 3,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("opens snapshot, reveals concealed field, diffs fields, confirms restore and allows undo", async () => {
    render(
      <TooltipProvider>
        <ItemDetailPane />
        <Toaster />
      </TooltipProvider>
    );

    // Wait for item detail to load
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "GitHub (v2)" })).toBeInTheDocument();
    });

    // Expand history
    const historyPane = screen.getByTestId("history");
    const toggleBtn = historyPane.querySelector("button")!;
    fireEvent.click(toggleBtn);

    // Version 1 link should appear
    await waitFor(() => {
      expect(screen.getByText(/Version 1/i)).toBeInTheDocument();
    });

    // Click version 1 to open snapshot
    fireEvent.click(screen.getByText(/Version 1/i));

    // Snapshot dialog opens
    await waitFor(() => {
      expect(screen.getByText(/Version 1 — GitHub \(v1\)/)).toBeInTheDocument();
    });

    // Verify diff badges
    expect(screen.getByTestId("diff-f2")).toHaveTextContent("changed");
    expect(screen.getByTestId("diff-f3")).toHaveTextContent("added");
    expect(screen.getByTestId("diff-f0")).toHaveTextContent("removed");

    // Concealed value is masked initially
    expect(screen.getAllByText("••••••••••").length).toBeGreaterThanOrEqual(1);

    // Click reveal button
    const revealBtn = screen.getByRole("button", { name: /reveal password/i });
    fireEvent.click(revealBtn);

    // Revealed value appears
    await waitFor(() => {
      expect(screen.getByText("FAKE-secret-revealed-pw")).toBeInTheDocument();
    });

    // Click "Restore this version"
    const restoreBtn = screen.getByTestId("restore-version-btn");
    fireEvent.click(restoreBtn);

    // Confirm step appears
    await waitFor(() => {
      expect(screen.getByTestId("restore-confirm-pane")).toBeInTheDocument();
      expect(screen.getByText(/Restore version 1\?/i)).toBeInTheDocument();
    });

    // Clicking Cancel in confirm step closes confirmation
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(screen.queryByTestId("restore-confirm-pane")).not.toBeInTheDocument();
    });

    // Click "Restore this version" again and confirm
    fireEvent.click(screen.getByTestId("restore-version-btn"));
    const confirmBtn = await screen.findByTestId("confirm-restore-btn");
    fireEvent.click(confirmBtn);

    // api.restoreItemVersion called with version 1
    await waitFor(() => {
      expect(api.restoreItemVersion).toHaveBeenCalledWith({
        vaultId: "v1",
        itemId: "item-1",
        version: 1,
      });
    });

    // Toast appears with Undo button
    await waitFor(() => {
      expect(screen.getByText(/Restored to version 1/i)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
    });

    // Click Undo
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));

    // api.restoreItemVersion called with pre-restore version (version 2)
    await waitFor(() => {
      expect(api.restoreItemVersion).toHaveBeenCalledWith({
        vaultId: "v1",
        itemId: "item-1",
        version: 2,
      });
    });
  });
});
