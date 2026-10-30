import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0,
    scrollToIndex: () => {},
    getVirtualItems: () => Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../../api/client";
import type { ItemSummary, WatchtowerReport } from "../../api/types";
import { translate, useI18n } from "../../i18n";
import { useApp } from "../../lib/store";
import { TooltipProvider } from "../../ui";
import { GeneralTab } from "../settings/tabs/GeneralTab";
import { useSettings } from "../settings/useSettings";
import { ItemList } from "./ItemList";
import { ROW_WARNINGS_KEY, rowFindingsMap, useBreachedIds, useRowWarnings } from "./rowFindings";
import { useItemsUI } from "./state";

const base: ItemSummary = {
  id: "a", vaultId: "v1", category: "login", title: "Clean", subtitle: "me", favorite: false, archived: false, deletedAt: null,
  tags: [], updatedAt: "Fri 2026-05-01 7:00 AM (UTC+07:00)", createdAt: "Mon 2024-01-01 7:00 AM (UTC+07:00)",
};
const items: ItemSummary[] = [
  base,
  { ...base, id: "b", title: "Mixed", subtitle: "octocat" },
  { ...base, id: "c", title: "Oldie", subtitle: "x" },
  { ...base, id: "d", title: "Single", subtitle: "" },
];
const report = (over: Partial<WatchtowerReport> = {}): WatchtowerReport => ({
  score: 80, weak: ["b", "c"], reused: [["b", "e"]], old: ["c"], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [],
  expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], ignored: [], checkedAt: "x", hibpChecked: false, ...over,
});
const settingsBase = { autoLockMinutes: 10, clipboardClearSecs: 30, theme: "dark" as const, language: "en" as const, biometricUnlock: false, quickAccessHotkey: "CmdOrCtrl+Shift+Space", hibpCheck: false };

function setup(r: WatchtowerReport = report({ weak: ["b", "c", "d"], old: ["c"] }), on = true) {
  vi.spyOn(api, "listItems").mockResolvedValue(items);
  const wt = vi.spyOn(api, "watchtower").mockResolvedValue(r);
  act(() => {
    useApp.setState({ locked: false, route: { name: "items", filter: { kind: "all" } }, selectedItemId: null, multiIds: [] });
    useI18n.setState({ lang: "en" });
    useItemsUI.setState({ items: [], loading: false, query: "", category: null });
    useSettings.setState({ settings: { ...settingsBase, listDensity: "comfortable" }, error: null });
    useRowWarnings.setState({ enabled: on });
    useBreachedIds.setState({ breached: [] });
  });
  return wt;
}

afterEach(() => { cleanup(); vi.restoreAllMocks(); localStorage.clear(); });

describe("rowFindingsMap", () => {
  it("is empty without a report and orders danger findings first", () => {
    expect(rowFindingsMap(null).size).toBe(0);
    const m = rowFindingsMap(report({ weak: ["b"], reused: [["b", "e"]], expiringSoon: ["e"] }));
    expect(m.get("b")).toEqual({ severity: "danger", kinds: ["reused", "weak"] });
    expect(m.get("e")).toEqual({ severity: "danger", kinds: ["reused", "expiringSoon"] });
    expect(m.get("zzz")).toBeUndefined();
  });
  it("uses warning severity when nothing dangerous is found, and skips rotation/2FA-available suggestions", () => {
    const m = rowFindingsMap(report({ weak: [], reused: [], old: ["c"], totpAvailable: ["f"], missingTotp: ["f", "g"], rotationDue: [{ itemId: "h", days: 1, everyDays: 90 } as any] }));
    expect(m.get("c")).toEqual({ severity: "warn", kinds: ["old"] });
    expect(m.get("g")).toEqual({ severity: "warn", kinds: ["missingTotp"] });
    expect(m.has("f")).toBe(false);
    expect(m.has("h")).toBe(false);
  });
  it("adds remembered breached ids (HIBP) as danger", () => {
    const m = rowFindingsMap(report({ weak: ["c"], old: [] }), ["c"]);
    expect(m.get("c")).toEqual({ severity: "danger", kinds: ["breached", "weak"] });
  });
});

describe("ItemList Watchtower status icon", () => {
  const render_ = () => render(<TooltipProvider><ItemList /></TooltipProvider>);

  it("marks only rows with findings, by severity, with a tooltip and an accessible name", async () => {
    setup();
    render_();
    await screen.findByText("Clean");
    await waitFor(() => expect(screen.getAllByTestId("finding-icon")).toHaveLength(3));
    const rows = screen.getAllByTestId("item-row");
    const icon = (i: number) => rows[i]!.querySelector('[data-testid="finding-icon"]') as HTMLElement | null;
    expect(icon(0)).toBeNull();
    expect(icon(1)!.dataset.severity).toBe("danger");
    expect(icon(1)!.getAttribute("title")).toBe("Reused password · Weak password");
    expect(icon(2)!.dataset.severity).toBe("warn");
    expect(icon(2)!.getAttribute("title")).toBe("Weak password · Old password");
    expect(icon(3)!.getAttribute("title")).toBe("Weak password");
    expect(rows[0]!.getAttribute("aria-label")).toBeNull();
    expect(rows[1]!.getAttribute("aria-label")).toBe("Mixed, octocat, 2 security issues");
    expect(rows[3]!.getAttribute("aria-label")).toBe("Single, 1 security issue");
    expect(icon(1)!.getAttribute("aria-hidden")).toBe("true");
    expect(icon(1)!.tabIndex).toBe(-1);
  });

  it("does not show ignored findings (they are absent from the report)", async () => {
    setup(report({ weak: [], reused: [], old: [], ignored: [{ itemId: "b", vaultId: "v1", title: "Mixed", kind: "weak", description: "weak" }] }));
    render_();
    await screen.findByText("Mixed");
    await waitFor(() => expect(api.watchtower).toHaveBeenCalled());
    expect(screen.queryByTestId("finding-icon")).toBeNull();
  });

  it("shows breached rows after a HIBP check ran, without a network lookup from the list", async () => {
    const wt = setup(report({ weak: [], reused: [], old: [] }));
    act(() => useBreachedIds.getState().remember(report({ breached: ["d"], hibpChecked: true })));
    render_();
    await screen.findByText("Single");
    await waitFor(() => expect(screen.getAllByTestId("finding-icon")).toHaveLength(1));
    expect(wt).toHaveBeenCalledWith({ hibp: false });
    expect(screen.getAllByTestId("finding-icon")[0]!.dataset.severity).toBe("danger");
  });

  it("toggle off hides icons and skips the audit", async () => {
    const wt = setup(undefined, false);
    render_();
    await screen.findByText("Clean");
    expect(screen.queryByTestId("finding-icon")).toBeNull();
    expect(wt).not.toHaveBeenCalled();
  });
});

describe("Settings toggle", () => {
  it("defaults on and persists off in localStorage", () => {
    act(() => { useRowWarnings.setState({ enabled: true }); useSettings.setState({ settings: { ...settingsBase }, error: null }); useI18n.setState({ lang: "en" }); });
    render(<TooltipProvider><GeneralTab /></TooltipProvider>);
    const sw = screen.getByTestId("row-warnings-switch");
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByLabelText(translate("en", "settings.rowWarnings"))).toBe(sw);
    fireEvent.click(sw);
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(localStorage.getItem(ROW_WARNINGS_KEY)).toBe("off");
    fireEvent.click(sw);
    expect(localStorage.getItem(ROW_WARNINGS_KEY)).toBeNull();
  });
});
