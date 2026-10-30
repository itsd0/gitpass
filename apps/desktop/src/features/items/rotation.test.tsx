import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock @tanstack/react-virtual so jsdom renders all rows
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0,
    scrollToIndex: () => {},
    getVirtualItems: () => Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../../api/client";
import { mock } from "../../api/mock";
import type { ItemDetail, ItemSummary, VaultInfo } from "../../api/types";
import { translate, useI18n } from "../../i18n";
import { defaultRoute, useApp } from "../../lib/store";
import { TooltipProvider, useToasts } from "../../ui";
import { ItemEditor } from "./ItemEditor";
import { ItemList } from "./ItemList";
import { useItemsUI } from "./state";
import { useSettings } from "../settings/useSettings";
import { ROTATION_CHOICES, rotationLabel } from "./rotation";

const t = (key: any, vars?: any) => translate("en", key, vars);
const vaults: VaultInfo[] = [{ id: "vault-1", name: "Personal", description: null, itemCount: 1, shared: false, members: [] }];

function detail(over: Partial<ItemDetail> = {}): ItemDetail {
  return {
    id: "item-1", vaultId: "vault-1", category: "login", title: "FAKE Bank", subtitle: "me@example.invalid", favorite: false, archived: false,
    deletedAt: null, tags: [], updatedAt: "x", createdAt: "x",
    sections: [{ id: "login", title: null, fields: [{ id: "username", label: "username", kind: "text", value: "me@example.invalid" }] }],
    notes: null, attachments: [], history: [], hasTotp: false, ...over,
  };
}

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("rotation pill labels", () => {
  it("labels due, soon (singular and plural) and nothing", () => {
    expect(ROTATION_CHOICES).toEqual([30, 90, 180, 365]);
    expect(rotationLabel({ state: "due", everyDays: 90, days: 120 }, t)).toBe("Due for rotation");
    expect(rotationLabel({ state: "soon", everyDays: 90, days: 1 }, t)).toBe("Rotate in 1 day");
    expect(rotationLabel({ state: "soon", everyDays: 90, days: 9 }, t)).toBe("Rotate in 9 days");
    expect(rotationLabel(null, t)).toBeNull();
    expect(rotationLabel(undefined, t)).toBeNull();
  });
  it("is translated in Burmese and Thai", () => {
    for (const lang of ["my", "th"] as const) {
      expect(translate(lang, "rotation.due")).not.toBe("Due for rotation");
      expect(translate(lang, "rotation.soonDays", { n: 9 })).toContain("9");
    }
  });
});

describe("ItemList rotation pill", () => {
  const base: ItemSummary = {
    id: "a", vaultId: "v1", category: "login", title: "Due Bank", subtitle: "me", favorite: false, archived: false, deletedAt: null,
    tags: [], updatedAt: "Fri 2026-05-01 7:00 AM (UTC+07:00)", createdAt: "Mon 2024-01-01 7:00 AM (UTC+07:00)",
  };
  const items: ItemSummary[] = [
    { ...base, rotateEveryDays: 90, rotation: { state: "due", everyDays: 90, days: 120 } },
    { ...base, id: "b", title: "Soon Mail", rotateEveryDays: 30, rotation: { state: "soon", everyDays: 30, days: 5 } },
    { ...base, id: "c", title: "Fine Forum", rotateEveryDays: 365 },
  ];
  const settingsBase = { autoLockMinutes: 10, clipboardClearSecs: 30, theme: "dark" as const, language: "en" as const, biometricUnlock: false, quickAccessHotkey: "CmdOrCtrl+Shift+Space", hibpCheck: false };
  function setup(density: "comfortable" | "compact") {
    vi.spyOn(api, "listItems").mockResolvedValue(items);
    act(() => {
      useApp.setState({ locked: false, route: { name: "items", filter: { kind: "all" } }, selectedItemId: null, multiIds: [] });
      useI18n.setState({ lang: "en" });
      useItemsUI.setState({ items: [], loading: false, query: "", category: null });
      useSettings.setState({ settings: { ...settingsBase, listDensity: density }, error: null });
    });
  }

  it("shows a pill only for logins that are due or due soon", async () => {
    setup("comfortable");
    render(<TooltipProvider><ItemList /></TooltipProvider>);
    await screen.findByText("Due Bank");
    const pills = screen.getAllByTestId("rotation-pill");
    expect(pills.map((p) => p.textContent)).toEqual(["Due for rotation", "Rotate in 5 days"]);
    expect(screen.getAllByTestId("rotation-pill")).toHaveLength(2);
  });

  it("collapses to a dot with the full label in compact density", async () => {
    setup("compact");
    render(<TooltipProvider><ItemList /></TooltipProvider>);
    await screen.findByText("Due Bank");
    const dots = screen.getAllByTestId("rotation-dot");
    expect(dots.map((d) => d.getAttribute("aria-label"))).toEqual(["Due for rotation", "Rotate in 5 days"]);
    expect(screen.queryByTestId("rotation-pill")).toBeNull();
  });
});

describe("ItemEditor rotation select", () => {
  beforeEach(() => {
    vi.spyOn(api, "passwordStrength").mockResolvedValue(80);
    act(() => {
      useApp.setState({ locked: false, route: defaultRoute, modal: null, editing: true });
      useI18n.setState({ lang: "en" });
      useToasts.setState({ list: [] });
    });
  });
  const renderEditor = (d: ItemDetail) => render(<TooltipProvider><ItemEditor detail={d} vaults={vaults} /></TooltipProvider>);

  it("offers the four intervals and saves a change through setRotation", async () => {
    vi.spyOn(api, "updateItem").mockResolvedValue({} as any);
    const setRotation = vi.spyOn(api, "setRotation").mockResolvedValue(undefined);
    renderEditor(detail());
    const select = screen.getByLabelText("Change password every…") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.textContent)).toEqual(["Never (no reminder)", "30 days", "90 days", "180 days", "365 days"]);
    expect(select.value).toBe("");
    fireEvent.change(select, { target: { value: "90" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(setRotation).toHaveBeenCalledWith({ vaultId: "vault-1", itemId: "item-1", days: 90 }));
  });

  it("clears the reminder, and skips setRotation when unchanged", async () => {
    vi.spyOn(api, "updateItem").mockResolvedValue({} as any);
    const setRotation = vi.spyOn(api, "setRotation").mockResolvedValue(undefined);
    const { unmount } = renderEditor(detail({ rotateEveryDays: 180 }));
    const select = screen.getByLabelText("Change password every…") as HTMLSelectElement;
    expect(select.value).toBe("180");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.updateItem).toHaveBeenCalled());
    expect(setRotation).not.toHaveBeenCalled();
    unmount();
    renderEditor(detail({ rotateEveryDays: 180 }));
    fireEvent.change(screen.getByLabelText("Change password every…"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(setRotation).toHaveBeenCalledWith({ vaultId: "vault-1", itemId: "item-1", days: null }));
  });

  it("is only shown for logins", () => {
    renderEditor(detail({ category: "secure_note" }));
    expect(screen.queryByLabelText("Change password every…")).toBeNull();
  });
});

describe("browser mock set_rotation", () => {
  it("rejects intervals other than 30, 90, 180 and 365", async () => {
    await expect((mock as any).set_rotation({ itemId: "does-not-exist", days: 45 })).rejects.toThrow(/30, 90, 180 or 365/);
  });
});
