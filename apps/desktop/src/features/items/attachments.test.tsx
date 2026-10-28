import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ItemDetailPane, dropInsideElement } from "./ItemDetail";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { useI18n } from "../../i18n";
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
  title: "GitHub Login",
  subtitle: "alice@example.com",
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: "2026-10-28T00:00:00Z",
  createdAt: "2026-10-20T00:00:00Z",
};

const initialDetail: ItemDetail = {
  ...mockSummary,
  sections: [
    {
      id: "sec-1",
      title: null,
      fields: [{ id: "username", kind: "text", label: "username", value: "alice@example.com" }],
    },
  ],
  notes: null,
  attachments: [
    { id: "att-1", name: "statement.pdf", size: 4096, mime: "application/pdf" },
  ],
  history: [{ version: 1, updatedAt: "2026-10-28T00:00:00Z" }],
  hasTotp: false,
  findings: [],
};

describe("ItemDetail attachments functionality", () => {
  let currentDetail: ItemDetail;

  beforeEach(() => {
    currentDetail = JSON.parse(JSON.stringify(initialDetail));
    useI18n.getState().setLang("en");
    useApp.setState({
      selectedItemId: "item-1",
      editing: false,
      itemsVersion: 0,
    });
    useItemsUI.setState({
      items: [mockSummary],
      loading: false,
    });
    useToasts.setState({ list: [] });

    vi.spyOn(api, "listVaults").mockResolvedValue([mockVault]);
    vi.spyOn(api, "listTags").mockResolvedValue([]);
    vi.spyOn(api, "getItem").mockImplementation(async () => JSON.parse(JSON.stringify(currentDetail)));
    vi.spyOn(api, "addAttachment").mockImplementation(async ({ path }) => {
      const name = path.split(/[\\/]/).pop() || "file";
      const newAtt = { id: "att-2", name, size: 2048, mime: "text/plain" };
      currentDetail.attachments.push(newAtt);
      return newAtt;
    });
    vi.spyOn(api, "saveAttachment").mockResolvedValue(undefined);
    vi.spyOn(api, "removeAttachment").mockImplementation(async ({ attachmentId }) => {
      currentDetail.attachments = currentDetail.attachments.filter((a) => a.id !== attachmentId);
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  const renderPane = () =>
    render(
      <TooltipProvider>
        <ItemDetailPane />
        <Toaster />
      </TooltipProvider>
    );

  it("renders attachments section with existing attachment and Add file button", async () => {
    renderPane();

    expect(await screen.findByText("statement.pdf")).toBeInTheDocument();
    expect(screen.getByText("4.0 KB")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add file…" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save statement.pdf" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove statement.pdf" })).toBeInTheDocument();
  });

  it("clicking Save attachment triggers api.saveAttachment and toast", async () => {
    renderPane();
    const saveBtn = await screen.findByRole("button", { name: "Save statement.pdf" });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(api.saveAttachment).toHaveBeenCalledWith(
        expect.objectContaining({ vaultId: "v1", itemId: "item-1", attachmentId: "att-1" })
      );
    });
    expect(await screen.findByText("Saved statement.pdf")).toBeInTheDocument();
  });

  it("adding a file via input calls api.addAttachment and updates list", async () => {
    renderPane();
    await screen.findByText("statement.pdf");

    const fileInput = screen.getByTestId("attachment-file-input");
    const file = new File(["dummy notes"], "notes.txt", { type: "text/plain" });
    fireEvent.change(fileInput, { target: { files: [file] } });

    await waitFor(() => {
      expect(api.addAttachment).toHaveBeenCalledWith(
        expect.objectContaining({ vaultId: "v1", itemId: "item-1", path: "notes.txt" })
      );
    });
    expect(await screen.findByText("Added notes.txt")).toBeInTheDocument();
    expect(await screen.findByText("notes.txt")).toBeInTheDocument();
  });

  it("drag-and-drop file onto detail pane adds attachment", async () => {
    renderPane();
    const pane = await screen.findByTestId("attachments-card");

    const file = new File(["dropped data"], "invoice.pdf", { type: "application/pdf" });
    fireEvent.dragOver(pane, { dataTransfer: { files: [file] } });
    expect(screen.getByText("Drop file here to attach")).toBeInTheDocument();

    fireEvent.drop(pane, { dataTransfer: { files: [file] } });

    await waitFor(() => {
      expect(api.addAttachment).toHaveBeenCalledWith(
        expect.objectContaining({ vaultId: "v1", itemId: "item-1", path: "invoice.pdf" })
      );
    });
    expect(await screen.findByText("Added invoice.pdf")).toBeInTheDocument();
  });

  it("remove attachment asks for confirmation and cancels cleanly", async () => {
    renderPane();
    const removeBtn = await screen.findByRole("button", { name: "Remove statement.pdf" });
    fireEvent.click(removeBtn);

    expect(await screen.findByRole("dialog", { name: "Remove attachment?" })).toBeInTheDocument();
    expect(screen.getByText(/Are you sure you want to remove "statement.pdf"/i)).toBeInTheDocument();

    // Cancel
    const cancelBtn = screen.getByRole("button", { name: "Cancel" });
    fireEvent.click(cancelBtn);

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(api.removeAttachment).not.toHaveBeenCalled();
    expect(screen.getByText("statement.pdf")).toBeInTheDocument();
  });

  it("confirming remove deletes attachment, shows toast, and renders empty state", async () => {
    renderPane();
    const removeBtn = await screen.findByRole("button", { name: "Remove statement.pdf" });
    fireEvent.click(removeBtn);

    const confirmBtn = await screen.findByRole("button", { name: "Remove" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(api.removeAttachment).toHaveBeenCalledWith({
        vaultId: "v1",
        itemId: "item-1",
        attachmentId: "att-1",
      });
    });

    expect(await screen.findByText("Removed statement.pdf")).toBeInTheDocument();
    expect(await screen.findByText("No attachments on this item")).toBeInTheDocument();
    expect(screen.queryByText("statement.pdf")).toBeNull();
  });

  it("keyboard navigation: Delete/Backspace on focused row triggers remove confirmation", async () => {
    renderPane();
    const row = await screen.findByRole("row", { name: "statement.pdf" });
    row.focus();
    expect(row).toHaveFocus();

    fireEvent.keyDown(row, { key: "Delete" });
    expect(await screen.findByRole("dialog", { name: "Remove attachment?" })).toBeInTheDocument();

    // Close with Escape
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    // Backspace also opens it
    fireEvent.keyDown(row, { key: "Backspace" });
    expect(await screen.findByRole("dialog", { name: "Remove attachment?" })).toBeInTheDocument();
  });
});

describe('dropInsideElement (native drop scope)', () => {
  const el = { getBoundingClientRect: () => ({ left: 100, right: 300, top: 50, bottom: 250 }) } as unknown as HTMLElement;
  it('accepts drops inside the detail pane, scaling physical pixels', () => {
    expect(dropInsideElement(el, { x: 150, y: 100 }, 1)).toBe(true);
    expect(dropInsideElement(el, { x: 400, y: 200 }, 2)).toBe(true);
  });
  it('rejects drops outside the pane, without a position or without a pane', () => {
    expect(dropInsideElement(el, { x: 50, y: 100 }, 1)).toBe(false);
    expect(dropInsideElement(el, { x: 150, y: 600 }, 2)).toBe(false);
    expect(dropInsideElement(el, undefined, 1)).toBe(false);
    expect(dropInsideElement(null, { x: 150, y: 100 }, 1)).toBe(false);
  });
});
