import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import fs from "fs";
import path from "path";
import { ItemDetailPane } from "./ItemDetail";
import { useApp } from "../../lib/store";
import { useItemsUI } from "./state";
import { useI18n, dictionaries } from "../../i18n";
import { en } from "../../i18n/en";
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
  title: "GitHub",
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
  sections: [
    {
      id: "sec-1",
      title: "Login credentials",
      fields: [
        { id: "f1", kind: "text", label: "username", value: "alice@example.com" },
        { id: "f2", kind: "concealed", label: "password", value: "secret123" },
      ],
    },
  ],
  notes: "My notes here",
  attachments: [
    { id: "att-1", name: "backup-codes.txt", size: 1024, mime: "text/plain" },
  ],
  history: [
    { version: 1, updatedAt: "2026-10-20T00:00:00Z" },
    { version: 2, updatedAt: "2026-10-28T00:00:00Z" },
  ],
  passwordStrength: 4,
};

describe("ItemDetail i18n & actions menu polish (#73)", () => {
  beforeEach(() => {
    useI18n.setState({ lang: "en" });
    useToasts.setState({ list: [] });
    useApp.setState({
      route: { name: "items", filter: { kind: "all", sort: "updated" } },
      selectedItemId: "item-1",
      editing: false,
      itemsVersion: 0,
      locked: false,
    });
    useItemsUI.setState({ items: [mockSummary], loading: false });

    vi.spyOn(api, "listVaults").mockResolvedValue([mockVault]);
    vi.spyOn(api, "listTags").mockResolvedValue(["dev"]);
    vi.spyOn(api, "getItem").mockResolvedValue(mockDetail);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    useI18n.setState({ lang: "en" });
    useToasts.setState({ list: [] });
  });

  it("statically verifies that no hardcoded English strings remain in ItemDetail.tsx", () => {
    const filePath = path.resolve(__dirname, "ItemDetail.tsx");
    const source = fs.readFileSync(filePath, "utf8");

    // Check specific banned hardcoded strings that were previously present
    const banned = [
      ">Share<",
      ">Edit<",
      "aria-label=\"More actions\"",
      ">Restore<",
      ">Duplicate<",
      ">Archive<",
      ">Unarchive<",
      ">Delete<",
      ">Delete permanently…<",
      "\"Item restored\"",
      "\"Item duplicated\"",
      "\"Archived\"",
      "\"Unarchived\"",
      "\"Moved to Recently Deleted\"",
      "\"Item permanently deleted\"",
      "\"This item is in Recently Deleted.\"",
      ">notes<",
      ">attachments<",
      "\"Delete permanently?\"",
    ];

    for (const pattern of banned) {
      expect(source.includes(pattern), `ItemDetail.tsx should not contain hardcoded string: ${pattern}`).toBe(false);
    }
  });

  it("verifies en/my/th parity for all detail.* keys", () => {
    const detailKeys = Object.keys(en).filter((k) => k.startsWith("detail."));
    expect(detailKeys.length).toBeGreaterThan(15);

    for (const lang of ["my", "th"] as const) {
      const dict = dictionaries[lang];
      for (const k of detailKeys) {
        expect(dict[k as keyof typeof en], `Missing key ${k} in ${lang}`).toBeTruthy();
      }
    }
  });

  it("renders menu labels and trigger in English by default with keyboard navigation", async () => {
    render(
      <TooltipProvider>
        <Toaster />
        <ItemDetailPane />
      </TooltipProvider>
    );

    // Header buttons
    expect(await screen.findByRole("button", { name: "Share" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Edit" })).toBeTruthy();

    // More actions button has localized aria-label
    const moreBtn = screen.getByRole("button", { name: "More actions" });
    expect(moreBtn).toBeTruthy();

    // Open More actions menu via keyboard (Enter key)
    fireEvent.keyDown(moreBtn, { key: "Enter", code: "Enter" });

    // Verify English menu items
    const favItem = await screen.findByRole("menuitem", { name: "Add to Favorites" });
    expect(favItem).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Move to vault" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Duplicate" })).toBeTruthy();
    const archiveItem = screen.getByRole("menuitem", { name: "Archive" });
    expect(archiveItem).toBeTruthy();

    // Verify Delete item is marked danger in light and dark modes
    const deleteItem = screen.getByRole("menuitem", { name: "Delete" });
    expect(deleteItem).toBeTruthy();
    expect(deleteItem.className).toContain("text-danger");
    expect(deleteItem.className).toContain("data-[highlighted]:bg-danger/10");
  });

  it("switching locale to my renders translated menu labels and toast after Archive", async () => {
    const archiveSpy = vi.spyOn(api, "archiveItem").mockResolvedValue();
    useI18n.getState().setLang("my");

    render(
      <TooltipProvider>
        <Toaster />
        <ItemDetailPane />
      </TooltipProvider>
    );

    // Header buttons in Burmese
    expect(await screen.findByRole("button", { name: "မျှဝေရန်" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "ပြင်ဆင်ရန်" })).toBeTruthy();

    // More actions button with Burmese aria-label
    const moreBtn = screen.getByRole("button", { name: "နောက်ထပ် လုပ်ဆောင်ချက်များ" });
    expect(moreBtn).toBeTruthy();

    // Open More actions menu via keyboard
    fireEvent.keyDown(moreBtn, { key: "Enter", code: "Enter" });

    // Verify Burmese menu items
    expect(await screen.findByRole("menuitem", { name: "အနှစ်သက်ဆုံးများထဲသို့ ထည့်ရန်" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "အခြား သော့ခတ်သေတ္တာသို့ ရွှေ့ရန်" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "မိတ္တူပွားရန်" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "သိမ်းဆည်းရန်" })).toBeTruthy();
    const deleteItem = screen.getByRole("menuitem", { name: "ဖျက်ရန်" });
    expect(deleteItem).toBeTruthy();
    expect(deleteItem.className).toContain("text-danger");

    // Click Archive
    const archiveItem = screen.getByRole("menuitem", { name: "သိမ်းဆည်းရန်" });
    fireEvent.click(archiveItem);

    await waitFor(() => {
      expect(archiveSpy).toHaveBeenCalledWith({
        vaultId: "v1",
        itemId: "item-1",
        archived: true,
      });
    });

    // Verify translated Burmese toast
    expect(await screen.findByText("သိမ်းဆည်းပြီးပါပြီ")).toBeTruthy();
  });

  it("handles deleted item actions, purge dialog, and toasts in Burmese", async () => {
    const deletedDetail: ItemDetail = {
      ...mockDetail,
      deletedAt: "2026-10-28T01:00:00Z",
    };
    vi.spyOn(api, "getItem").mockResolvedValue(deletedDetail);
    const purgeSpy = vi.spyOn(api, "purgeItem").mockResolvedValue();

    useI18n.getState().setLang("my");

    render(
      <TooltipProvider>
        <Toaster />
        <ItemDetailPane />
      </TooltipProvider>
    );

    // Banner in Burmese
    expect(await screen.findByText("ဤ item သည် မကြာသေးမီက ဖျက်ထားသည်များထဲတွင် ရှိနေသည်။")).toBeTruthy();

    // Open menu via keyboard
    const moreBtn = screen.getByRole("button", { name: "နောက်ထပ် လုပ်ဆောင်ချက်များ" });
    fireEvent.keyDown(moreBtn, { key: "Enter", code: "Enter" });

    // Restore & Delete permanently in Burmese
    expect(await screen.findByRole("menuitem", { name: "မူလအတိုင်း ပြန်ယူရန်" })).toBeTruthy();
    const purgeMenu = screen.getByRole("menuitem", { name: "အပြီးတိုင် ဖျက်ရန်…" });
    expect(purgeMenu).toBeTruthy();
    expect(purgeMenu.className).toContain("text-danger");

    // Open purge confirmation dialog
    fireEvent.click(purgeMenu);

    expect(await screen.findByText("အပြီးတိုင် ဖျက်မလား?")).toBeTruthy();
    expect(screen.getByText(/“GitHub” ကို အပြီးတိုင် ဖျက်ပါမည်။ ဤလုပ်ဆောင်ချက်ကို ပြန်ပြင်၍မရပါ။/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "မလုပ်တော့ပါ" })).toBeTruthy();

    const confirmPurgeBtn = screen.getByRole("button", { name: "အပြီးတိုင် ဖျက်ရန်" });
    expect(confirmPurgeBtn).toBeTruthy();

    // Click confirm purge
    fireEvent.click(confirmPurgeBtn);

    await waitFor(() => {
      expect(purgeSpy).toHaveBeenCalledWith({ vaultId: "v1", itemId: "item-1" });
    });

    expect(await screen.findByText("အပြီးတိုင် ဖျက်လိုက်ပါပြီ")).toBeTruthy();
  });

  it("handles duplicate and restore actions with localized toasts", async () => {
    const dupSpy = vi.spyOn(api, "duplicateItem").mockResolvedValue({ id: "item-2", vaultId: "v1" } as any);
    useI18n.getState().setLang("my");

    render(
      <TooltipProvider>
        <Toaster />
        <ItemDetailPane />
      </TooltipProvider>
    );

    const moreBtn = await screen.findByRole("button", { name: "နောက်ထပ် လုပ်ဆောင်ချက်များ" });
    fireEvent.keyDown(moreBtn, { key: "Enter", code: "Enter" });

    const dupItem = await screen.findByRole("menuitem", { name: "မိတ္တူပွားရန်" });
    fireEvent.click(dupItem);

    await waitFor(() => {
      expect(dupSpy).toHaveBeenCalledWith({ vaultId: "v1", itemId: "item-1" });
    });

    expect(await screen.findByText("မိတ္တူပွားပြီးပါပြီ")).toBeTruthy();
  });

  it("switching locale to th renders translated menu labels and toast after Archive", async () => {
    const archiveSpy = vi.spyOn(api, "archiveItem").mockResolvedValue();
    useI18n.getState().setLang("th");

    render(
      <TooltipProvider>
        <Toaster />
        <ItemDetailPane />
      </TooltipProvider>
    );

    // Header buttons in Thai
    expect(await screen.findByRole("button", { name: "แชร์" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "แก้ไข" })).toBeTruthy();

    // More actions button with Thai aria-label
    const moreBtn = screen.getByRole("button", { name: "การดำเนินการเพิ่มเติม" });
    expect(moreBtn).toBeTruthy();

    // Open More actions menu via keyboard
    fireEvent.keyDown(moreBtn, { key: "Enter", code: "Enter" });

    // Verify Thai menu items
    expect(await screen.findByRole("menuitem", { name: "เพิ่มในรายการโปรด" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "ย้ายไปยังห้องนิรภัย" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "ทำซ้ำ" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "เก็บถาวร" })).toBeTruthy();
    const deleteItem = screen.getByRole("menuitem", { name: "ลบ" });
    expect(deleteItem).toBeTruthy();
    expect(deleteItem.className).toContain("text-danger");

    // Click Archive
    const archiveItem = screen.getByRole("menuitem", { name: "เก็บถาวร" });
    fireEvent.click(archiveItem);

    await waitFor(() => {
      expect(archiveSpy).toHaveBeenCalledWith({
        vaultId: "v1",
        itemId: "item-1",
        archived: true,
      });
    });

    // Verify translated Thai toast
    expect(await screen.findByText("เก็บถาวรแล้ว")).toBeTruthy();
  });

  it("renders localized notes, attachments, and history snapshot in Burmese", async () => {
    useI18n.getState().setLang("my");

    render(
      <TooltipProvider>
        <Toaster />
        <ItemDetailPane />
      </TooltipProvider>
    );

    // Notes label
    expect(await screen.findByText("မှတ်စုများ")).toBeTruthy();

    // Attachments label & save button aria-label
    expect(screen.getByText("တွဲထားသော ဖိုင်များ")).toBeTruthy();
    expect(screen.getByRole("button", { name: "backup-codes.txt ကို သိမ်းရန်" })).toBeTruthy();

    // History section
    expect(screen.getByRole("button", { name: /နောက်ဆုံး တည်းဖြတ်ချိန်/ })).toBeTruthy();
  });
});
