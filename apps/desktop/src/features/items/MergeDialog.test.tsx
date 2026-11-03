import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MergeDialog } from "./MergeDialog";
import { api } from "../../api/client";
import { Toaster, TooltipProvider, useToasts } from "../../ui";
import type { ItemDetail, ItemSummary, MergeDuplicatePreview, MergeDuplicateResult } from "../../api/types";

const summaryA: ItemSummary = {
  id: "a", vaultId: "v1", category: "login", title: "Example Mail A", subtitle: "alice@example.invalid",
  favorite: false, archived: false, deletedAt: null, tags: ["work"],
  updatedAt: "2026-10-20T00:00:00Z", createdAt: "2026-10-20T00:00:00Z",
  url: "https://example.invalid", lastUsed: 1700000000,
};
const summaryB: ItemSummary = {
  id: "b", vaultId: "v1", category: "login", title: "Example Mail B", subtitle: "alice@example.invalid",
  favorite: false, archived: false, deletedAt: null, tags: ["personal"],
  updatedAt: "2026-10-28T00:00:00Z", createdAt: "2026-10-20T00:00:00Z",
  url: "https://other.example.invalid", lastUsed: 1790000000,
};
const detail = (s: ItemSummary, hasTotp: boolean): ItemDetail => ({
  ...s, version: 2, sections: [], notes: null, attachments: [], history: [], hasTotp,
});
const previewFor = (keepId: string): MergeDuplicatePreview => ({
  keepId, otherId: keepId === "a" ? "b" : "a",
  mergedTitle: "Example Mail", mergedUrl: "https://example.invalid", mergedTags: ["work", "personal"],
  report: {
    passwordFromOther: false, passwordMovedToHistory: true, totpFromOther: keepId === "a",
    urlsAdded: 1, tagsAdded: keepId === "a" ? ["personal"] : ["work"], relatedAdded: 0,
    notesJoined: false, fieldsCopied: [], fieldsConflicted: [], attachmentsAdded: 0, historyEntries: 2,
  },
});
const mergeResult: MergeDuplicateResult = {
  kept: summaryB, otherId: "a", otherTitle: "Example Mail",
  keepVersionBefore: 1, keepVersion: 3, report: previewFor("b").report,
};

describe("MergeDialog", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useToasts.setState({ list: [] });
    vi.spyOn(api, "getItem").mockImplementation(({ itemId }) =>
      Promise.resolve(detail(itemId === "a" ? summaryA : summaryB, itemId === "b")));
    vi.spyOn(api, "previewMergeDuplicates").mockImplementation(({ keepId }) =>
      Promise.resolve(previewFor(keepId)));
    vi.spyOn(api, "mergeDuplicates").mockResolvedValue(mergeResult);
    vi.spyOn(api, "restoreItem").mockResolvedValue(undefined);
    vi.spyOn(api, "restoreItemVersion").mockResolvedValue(detail(summaryB, true));
  });
  afterEach(() => { cleanup(); });

  // Stateful harness: the dialog really closes on confirm, so the toast
  // leaves Radix's aria-hidden dialog subtree and stays role-visible.
  const open = (onOpenChange = vi.fn()) => {
    const Harness = () => {
      const [open, setOpen] = useState(true);
      return (
        <TooltipProvider>
          <MergeDialog
            open={open}
            onOpenChange={(o) => { setOpen(o); onOpenChange(o); }}
            vaultId="v1" itemA={summaryA} itemB={summaryB}
          />
          <Toaster />
        </TooltipProvider>
      );
    };
    render(<Harness />);
    return onOpenChange;
  };

  it("defaults the keep radio to the most recently used item and shows the preview", async () => {
    open();
    await waitFor(() => expect(api.previewMergeDuplicates).toHaveBeenCalledWith(
      expect.objectContaining({ vaultId: "v1", keepId: "b", otherId: "a" })));
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(2);
    // Columns stay fixed to their item: the B column (most recently used) is checked.
    expect((within(screen.getByTestId("merge-column-b")).getByRole("radio") as HTMLInputElement).checked).toBe(true);
    expect((within(screen.getByTestId("merge-column-a")).getByRole("radio") as HTMLInputElement).checked).toBe(false);
    expect(screen.getByTestId("merge-column-a")).toHaveTextContent("Example Mail A");
    expect(screen.getByTestId("merge-column-b")).toHaveTextContent("Example Mail B");
    await waitFor(() => expect(screen.getByText(/Tags: work, personal/)).toBeInTheDocument());
    expect(screen.getByText(/extra website saved/)).toBeInTheDocument();
  });

  it("switching keep re-requests the preview, and confirm merges with an Undo toast", async () => {
    const onOpenChange = open();
    await waitFor(() => expect(api.previewMergeDuplicates).toHaveBeenCalledWith(
      expect.objectContaining({ keepId: "b" })));
    const radios = screen.getAllByRole("radio");
    fireEvent.click(radios[0]);
    await waitFor(() => expect(api.previewMergeDuplicates).toHaveBeenCalledWith(
      expect.objectContaining({ keepId: "a", otherId: "b" })));
    expect((within(screen.getByTestId("merge-column-a")).getByRole("radio") as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await waitFor(() => expect(api.mergeDuplicates).toHaveBeenCalledWith(
      { vaultId: "v1", keepId: "a", otherId: "b" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument());
    expect(onOpenChange).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(api.restoreItem).toHaveBeenCalledWith({ vaultId: "v1", itemId: "b" }));
    await waitFor(() => expect(api.restoreItemVersion).toHaveBeenCalledWith(
      { vaultId: "v1", itemId: "a", version: 1 }));
  });
});
