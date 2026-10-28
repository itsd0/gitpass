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

const mockItemWithFindings: Detail = {
  ...mockItemWithIgnored,
  id: "item-3",
  title: "Weak Forum",
  ignoredFindings: [],
  changePasswordUrl: "https://example.com/.well-known/change-password",
  findings: [
    { kind: "weak", description: "weak password: too short (6 < 12)" },
    { kind: "insecure_url", description: "saved with unencrypted HTTP URL (example.com)" },
  ],
};

describe("ItemDetail Watchtower findings banner", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listVaults").mockResolvedValue([mockVault]);
    vi.spyOn(api, "listTags").mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  it("shows findings with a Change password action", async () => {
    const openSpy = vi.spyOn(api, "openChangePassword").mockResolvedValue();
    vi.spyOn(api, "getItem").mockResolvedValue(mockItemWithFindings);
    vi.spyOn(api, "listItems").mockResolvedValue([mockItemWithFindings as ItemSummary]);

    useItemsUI.setState({ items: [mockItemWithFindings as any], loading: false });
    useApp.setState({ selectedItemId: "item-3", editing: false });

    render(<TooltipProvider><ItemDetailPane /></TooltipProvider>);

    const banner = await screen.findByTestId("watchtower-findings");
    expect(banner.textContent).toContain("Watchtower found issues with this item");
    expect(banner.textContent).toContain("weak password: too short (6 < 12)");
    expect(banner.textContent).toContain("saved with unencrypted HTTP URL (example.com)");

    fireEvent.click(screen.getByRole("button", { name: "Change password for Weak Forum" }));
    expect(openSpy).toHaveBeenCalledWith({ vaultId: "v-1", itemId: "item-3" });
  });

  it("shows the banner without the action when no password finding applies", async () => {
    const noAction: Detail = {
      ...mockItemWithFindings,
      id: "item-4",
      title: "No TOTP Login",
      changePasswordUrl: undefined,
      findings: [{ kind: "missing_totp", description: "has a URL but no TOTP configured" }],
    };
    vi.spyOn(api, "getItem").mockResolvedValue(noAction);
    vi.spyOn(api, "listItems").mockResolvedValue([noAction as ItemSummary]);

    useItemsUI.setState({ items: [noAction as any], loading: false });
    useApp.setState({ selectedItemId: "item-4", editing: false });

    render(<TooltipProvider><ItemDetailPane /></TooltipProvider>);

    expect(await screen.findByTestId("watchtower-findings")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Change password/i })).toBeNull();
  });

  it("does not show the banner without findings", async () => {
    const clean: Detail = { ...mockItemClean, id: "item-5", title: "Clean Two" };
    vi.spyOn(api, "getItem").mockResolvedValue(clean);
    vi.spyOn(api, "listItems").mockResolvedValue([clean as ItemSummary]);

    useItemsUI.setState({ items: [clean as any], loading: false });
    useApp.setState({ selectedItemId: "item-5", editing: false });

    render(<TooltipProvider><ItemDetailPane /></TooltipProvider>);

    await screen.findAllByText("Clean Two");
    expect(screen.queryByTestId("watchtower-findings")).toBeNull();
  });
});
