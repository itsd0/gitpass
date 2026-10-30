import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Unlock } from "./Unlock";
import { Sidebar } from "./Sidebar";
import { useApp } from "../lib/store";
import { useI18n, translate } from "../i18n";
import { api } from "../api/client";
import { TooltipProvider } from "../ui";

describe("Unlock localization (en, my, th)", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useI18n.getState().setLang("en");
    useApp.setState({
      locked: true,
      account: null,
      sidebarCollapsed: false,
      route: { name: "items", filter: { kind: "all" } },
    });
  });

  afterEach(() => {
    cleanup();
    useI18n.getState().setLang("en");
  });

  it("renders unlock screen with localized headings and aria-labels in English", async () => {
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "/tmp/fake-vault",
      account: null,
      vaultExists: true,
      biometricAvailable: true,
    });

    useI18n.getState().setLang("en");
    await act(async () => {
      render(<Unlock />);
    });

    expect(screen.getByRole("heading", { name: "Unlock gitpass" })).toBeTruthy();
    expect(screen.getByText("Master password")).toBeTruthy();
    expect(screen.getByPlaceholderText("Enter your password")).toBeTruthy();
    const unlockButtons = screen.getAllByRole("button", { name: "Unlock" });
    expect(unlockButtons.length).toBe(2);
    expect(screen.getByRole("button", { name: /Unlock with Touch ID/i })).toBeTruthy();

    // Toggle advanced fields
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Vault folder & Secret Key/i }));
    });

    expect(screen.getByLabelText("Vault folder")).toBeTruthy();
    expect(screen.getByLabelText("Secret Key")).toBeTruthy();
    expect(screen.getByPlaceholderText("Secret Key (A3-XXXXXX-…), optional")).toBeTruthy();
  });

  it("renders unlock screen with localized Burmese strings and aria-labels", async () => {
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "/tmp/fake-vault",
      account: null,
      vaultExists: true,
      biometricAvailable: true,
    });

    useI18n.getState().setLang("my");
    await act(async () => {
      render(<Unlock />);
    });

    // Heading: gitpass ကို သော့ဖွင့်ပါ
    expect(screen.getByRole("heading", { name: translate("my", "unlock.title") })).toBeTruthy();
    expect(screen.getByText(translate("my", "unlock.masterPassword"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("my", "unlock.masterPasswordPlaceholder"))).toBeTruthy();
    // Submit buttons aria-label & text: သော့ဖွင့်ပါ
    const unlockBtns = screen.getAllByRole("button", { name: translate("my", "unlock.submit") });
    expect(unlockBtns.length).toBe(2);
    expect(screen.getByRole("button", { name: new RegExp(translate("my", "unlock.touchId"), "i") })).toBeTruthy();

    // Toggle advanced
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: new RegExp(translate("my", "unlock.showAdvanced"), "i") }));
    });

    expect(screen.getByLabelText(translate("my", "unlock.vaultFolder"))).toBeTruthy();
    expect(screen.getByLabelText(translate("my", "unlock.secretKey"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("my", "unlock.secretKeyPlaceholder"))).toBeTruthy();
  });

  it("renders unlock screen with localized Thai strings and aria-labels", async () => {
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "/tmp/fake-vault",
      account: null,
      vaultExists: true,
      biometricAvailable: true,
    });

    useI18n.getState().setLang("th");
    await act(async () => {
      render(<Unlock />);
    });

    // Heading: ปลดล็อก gitpass
    expect(screen.getByRole("heading", { name: translate("th", "unlock.title") })).toBeTruthy();
    expect(screen.getByText(translate("th", "unlock.masterPassword"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("th", "unlock.masterPasswordPlaceholder"))).toBeTruthy();
    // Submit buttons aria-label & text: ปลดล็อก
    const unlockBtns = screen.getAllByRole("button", { name: translate("th", "unlock.submit") });
    expect(unlockBtns.length).toBe(2);
    expect(screen.getByRole("button", { name: new RegExp(translate("th", "unlock.touchId"), "i") })).toBeTruthy();

    // Toggle advanced
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: new RegExp(translate("th", "unlock.showAdvanced"), "i") }));
    });

    expect(screen.getByLabelText(translate("th", "unlock.vaultFolder"))).toBeTruthy();
    expect(screen.getByLabelText(translate("th", "unlock.secretKey"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("th", "unlock.secretKeyPlaceholder"))).toBeTruthy();
  });

  it("interpolates {path} and localizes create-vault screen in Burmese and Thai when vault does not exist", async () => {
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "/custom/my-vault",
      account: null,
      vaultExists: false,
      biometricAvailable: false,
    });

    // Burmese test
    useI18n.getState().setLang("my");
    const { unmount } = render(<Unlock />);

    await act(async () => {});

    expect(screen.getByRole("heading", { name: translate("my", "unlock.createTitle") })).toBeTruthy();
    expect(screen.getByText("/custom/my-vault တွင် သော့ခတ်သေတ္တာ မတွေ့ပါ")).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("my", "unlock.newPasswordPlaceholder"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("my", "unlock.confirmPasswordPlaceholder"))).toBeTruthy();
    expect(screen.getByText(translate("my", "unlock.storeSecretKeyLocal"))).toBeTruthy();
    expect(screen.getByRole("button", { name: new RegExp(translate("my", "unlock.createVault"), "i") })).toBeTruthy();

    unmount();

    // Thai test
    useI18n.getState().setLang("th");
    render(<Unlock />);
    await act(async () => {});

    expect(screen.getByRole("heading", { name: translate("th", "unlock.createTitle") })).toBeTruthy();
    expect(screen.getByText("ไม่พบห้องนิรภัยที่ /custom/my-vault")).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("th", "unlock.newPasswordPlaceholder"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("th", "unlock.confirmPasswordPlaceholder"))).toBeTruthy();
    expect(screen.getByText(translate("th", "unlock.storeSecretKeyLocal"))).toBeTruthy();
    expect(screen.getByRole("button", { name: new RegExp(translate("th", "unlock.createVault"), "i") })).toBeTruthy();
  });

  it("shows localized validation errors in create-vault mode", async () => {
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "/tmp/vault",
      account: null,
      vaultExists: false,
      biometricAvailable: false,
    });

    useI18n.getState().setLang("th");
    render(<Unlock />);
    await act(async () => {});

    // Short password error (< 8 chars)
    const pw1 = screen.getByPlaceholderText(translate("th", "unlock.newPasswordPlaceholder"));
    const pw2 = screen.getByPlaceholderText(translate("th", "unlock.confirmPasswordPlaceholder"));

    await act(async () => {
      fireEvent.change(pw1, { target: { value: "short" } });
      fireEvent.change(pw2, { target: { value: "short" } });
    });

    await act(async () => {
      fireEvent.submit(screen.getByTestId("create-vault").querySelector("form")!);
    });

    expect(screen.getByRole("alert").textContent).toBe(translate("th", "unlock.errShortPassword"));

    // Password mismatch
    await act(async () => {
      fireEvent.change(pw1, { target: { value: "password123" } });
      fireEvent.change(pw2, { target: { value: "password999" } });
    });

    await act(async () => {
      fireEvent.submit(screen.getByTestId("create-vault").querySelector("form")!);
    });

    expect(screen.getByRole("alert").textContent).toBe(translate("th", "unlock.errPasswordMismatch"));
  });

  it("localizes the Secret Key display screen after vault creation in Burmese and Thai", async () => {
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "/tmp/vault",
      account: null,
      vaultExists: false,
      biometricAvailable: false,
    });
    vi.spyOn(api, "createAccount").mockResolvedValue({
      secretKey: "A3-DEMO-SECRET-KEY",
    });

    useI18n.getState().setLang("my");
    render(<Unlock />);
    await act(async () => {});

    const pw1 = screen.getByPlaceholderText(translate("my", "unlock.newPasswordPlaceholder"));
    const pw2 = screen.getByPlaceholderText(translate("my", "unlock.confirmPasswordPlaceholder"));

    await act(async () => {
      fireEvent.change(pw1, { target: { value: "longmasterpassword" } });
      fireEvent.change(pw2, { target: { value: "longmasterpassword" } });
    });

    await act(async () => {
      fireEvent.submit(screen.getByTestId("create-vault").querySelector("form")!);
    });

    expect(screen.getByRole("heading", { name: translate("my", "unlock.saveSecretKey") })).toBeTruthy();
    expect(screen.getByText(translate("my", "unlock.saveSecretKeyDesc"))).toBeTruthy();
    expect(screen.getByRole("button", { name: translate("my", "unlock.copySecretKey") })).toBeTruthy();
    expect(screen.getByRole("button", { name: new RegExp(translate("my", "unlock.printEmergencyKit"), "i") })).toBeTruthy();
    expect(screen.getByText(translate("my", "unlock.savedConfirmation"))).toBeTruthy();
    expect(screen.getByRole("button", { name: translate("my", "unlock.continue") })).toBeTruthy();
  });

  it("localizes sidebar controls and new vault dialog in Burmese and Thai", async () => {
    vi.spyOn(api, "listVaults").mockResolvedValue([
      { id: "v1", name: "Personal", shared: false, itemCount: 5, color: null, icon: null },
      { id: "v2", name: "Shared Team", shared: true, itemCount: 10, color: null, icon: null },
    ]);
    vi.spyOn(api, "listTags").mockResolvedValue([]);

    useI18n.getState().setLang("my");
    await act(async () => {
      render(<TooltipProvider><Sidebar /></TooltipProvider>);
    });

    // Collapse sidebar button aria-label
    expect(screen.getByRole("button", { name: translate("my", "sidebar.collapse") })).toBeTruthy();
    // New vault button aria-label in section header
    expect(screen.getByRole("button", { name: translate("my", "sidebar.newVault") })).toBeTruthy();
    // Shared vault icon aria-label
    expect(screen.getByLabelText(translate("my", "sidebar.sharedVault"))).toBeTruthy();

    // Click new vault button to open dialog
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: translate("my", "sidebar.newVault") }));
    });

    // Dialog title & description
    expect(screen.getByRole("heading", { name: translate("my", "vault.newTitle") })).toBeTruthy();
    expect(screen.getByText(translate("my", "vault.newDesc"))).toBeTruthy();
    expect(screen.getByPlaceholderText(translate("my", "vault.namePlaceholder"))).toBeTruthy();
    expect(screen.getByRole("button", { name: translate("my", "common.cancel") })).toBeTruthy();
    expect(screen.getByRole("button", { name: translate("my", "vault.create") })).toBeTruthy();
  });
  it("displays live countdown and disables unlock button when throttled", async () => {
    vi.useFakeTimers();
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "~/.gitpass",
      account: null,
      vaultExists: true,
      throttledSecs: 3,
    });

    await act(async () => {
      render(<TooltipProvider><Unlock /></TooltipProvider>);
    });

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("Too many attempts. Try again in 3 s");

    // Unlock buttons should be disabled
    const submitBtns = screen.getAllByRole("button", { name: "Unlock" });
    expect(submitBtns.length).toBe(2);
    expect((submitBtns[0] as HTMLButtonElement).disabled).toBe(true);
    expect((submitBtns[1] as HTMLButtonElement).disabled).toBe(true);

    // Advance 1s
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByRole("alert").textContent).toBe("Too many attempts. Try again in 2 s");

    // Advance 2s more -> countdown ends
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.queryByText(/Too many attempts/)).toBeNull();

    vi.useRealTimers();
  });

  it("handles throttled error rejection from unlock submit with countdown and localized text", async () => {
    vi.useFakeTimers();
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "~/.gitpass",
      account: null,
      vaultExists: true,
    });
    vi.spyOn(api, "unlock").mockRejectedValue(new Error("throttled: try again in 2 s"));

    useI18n.getState().setLang("my");
    await act(async () => {
      render(<TooltipProvider><Unlock /></TooltipProvider>);
    });

    const pwInput = screen.getByPlaceholderText(translate("my", "unlock.masterPasswordPlaceholder"));
    await act(async () => {
      fireEvent.change(pwInput, { target: { value: "some-password" } });
    });

    const submitBtns = screen.getAllByRole("button", { name: translate("my", "unlock.submit") });
    await act(async () => {
      fireEvent.click(submitBtns[1]);
    });

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe(translate("my", "unlock.throttled", { n: 2 }));
    expect((submitBtns[1] as HTMLButtonElement).disabled).toBe(true);

    // Advance 2 seconds
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.queryByText(/ကြိုးစားမှု/)).toBeNull();

    vi.useRealTimers();
  });
});
