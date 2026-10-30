import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  Archive,
  CalendarClock,
  CalendarX,
  ChevronRight,
  Clock,
  Copy,
  ExternalLink,
  EyeOff,
  Globe,
  History,
  Key,
  KeyRound,
  Layers,
  MoreVertical,
  Pencil,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Smartphone,
} from "lucide-react";
import type { ItemSummary, WatchtowerReport } from "../../api/types";
import { useBreachedIds } from "../items/rowFindings";
import { api } from "../../api/client";
import { SiteIcon } from "../../components/SiteIcon";
import { useT, useI18n, type Key as TranslationKey } from "../../i18n";
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Skeleton,
  toast,
} from "../../ui";
import { useApp } from "../../lib/store";
import { useSettings } from "../settings/useSettings";
import { loadHistory, recordReport, type ScorePoint } from "./history";
import { CHECKUP_RELOAD_EVENT } from "./SecurityCheckup";
import { buildCheckupQueue } from "./checkup";
import { ScoreHistory } from "./ScoreHistory";
import { gaugeDash, issueIds, scoreBand, type IssueKey } from "./score";
import { cn } from "../../lib/utils";

export function ScoreGauge({ score }: { score: number }) {
  const t = useT();
  const b = scoreBand(score);
  const d = gaugeDash(score);
  const label = t(`watchtower.score.${b.labelKey}` as 'watchtower.score.excellent');
  return (
    <div
      className="relative h-40 w-40"
      role="img"
      aria-label={t('watchtower.score.aria', { score: b.score, label })}
    >
      <svg viewBox="0 0 120 120" className="h-full w-full rotate-[135deg]">
        <circle
          cx="60"
          cy="60"
          r="52"
          fill="none"
          stroke="#262626"
          strokeWidth="10"
          strokeDasharray={`${d.arc} ${d.circumference}`}
          strokeLinecap="round"
        />
        <circle
          cx="60"
          cy="60"
          r="52"
          fill="none"
          stroke={b.color}
          strokeWidth="10"
          strokeDasharray={`${d.filled} ${d.circumference}`}
          strokeLinecap="round"
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span data-testid="score" className="text-4xl font-bold">
          {b.score}
        </span>
        <span className="text-sm" style={{ color: b.color }}>
          {label}
        </span>
      </div>
    </div>
  );
}

const CARDS: { key: IssueKey; titleKey: TranslationKey; descKey: TranslationKey; Icon: typeof AlertTriangle }[] = [
  { key: "weak", titleKey: "watchtower.weakPasswords", descKey: "watchtower.weakPasswordsDesc", Icon: AlertTriangle },
  { key: "reused", titleKey: "watchtower.reusedPasswords", descKey: "watchtower.reusedPasswordsDesc", Icon: Copy },
  { key: "old", titleKey: "watchtower.oldPasswords", descKey: "watchtower.oldPasswordsDesc", Icon: Clock },
  { key: "breached", titleKey: "watchtower.breachedPasswords", descKey: "watchtower.breachedPasswordsDesc", Icon: ShieldAlert },
  { key: "missingTotp", titleKey: "watchtower.missingTotp", descKey: "watchtower.missingTotpDesc", Icon: Smartphone },
];

const ISSUE_TO_KIND: Record<IssueKey, string> = {
  weak: "weak",
  reused: "reused",
  old: "old",
  breached: "pwned",
  missingTotp: "missing_totp",
  duplicates: "duplicate",
  totpAvailable: "totp_available",
  passkeyAvailable: "passkey_available",
  expired: "expired",
  expiringSoon: "expiring_soon",
  insecureUrls: "insecure_url",
  weakSshKey: "weak_ssh_key",
  unused: "unused",
  rotationDue: "rotation_due",
  rotationSoon: "rotation_soon",
};

export function WatchtowerPage() {
  const t = useT();
  const lang = useI18n((s) => s.lang);
  const cards = [
    ...CARDS,
    { key: "duplicates" as const, titleKey: "watchtower.duplicates" as const, descKey: "watchtower.duplicatesDesc" as const, Icon: Layers },
    { key: "totpAvailable" as const, titleKey: "watchtower.totpAvailable" as const, descKey: "watchtower.totpAvailableDesc" as const, Icon: Smartphone },
    { key: "passkeyAvailable" as const, titleKey: "watchtower.passkeyAvailable" as const, descKey: "watchtower.passkeyAvailableDesc" as const, Icon: KeyRound },
    { key: "expired" as const, titleKey: "watchtower.expired" as const, descKey: "watchtower.expiredDesc" as const, Icon: CalendarX },
    { key: "expiringSoon" as const, titleKey: "watchtower.expiringSoon" as const, descKey: "watchtower.expiringSoonDesc" as const, Icon: CalendarClock },
    { key: "insecureUrls" as const, titleKey: "watchtower.insecureUrls" as const, descKey: "watchtower.insecureUrlsDesc" as const, Icon: Globe },
    { key: "weakSshKey" as const, titleKey: "watchtower.weakSshKey" as const, descKey: "watchtower.weakSshKeyDesc" as const, Icon: Key },
    { key: "unused" as const, titleKey: "watchtower.unusedItems" as const, descKey: "watchtower.unusedItemsDesc" as const, Icon: History },
    { key: "rotationDue" as const, titleKey: "watchtower.rotationDue" as const, descKey: "watchtower.rotationDueDesc" as const, Icon: RefreshCw },
    { key: "rotationSoon" as const, titleKey: "watchtower.rotationSoon" as const, descKey: "watchtower.rotationSoonDesc" as const, Icon: CalendarClock },
  ].map((c) => ({ ...c, title: t(c.titleKey), desc: t(c.descKey) }));
  const { selectItem, setRoute, setEditing } = useApp();
  const settings = useSettings((st) => st.settings);
  const hibp = !!settings?.hibpCheck;
  const unusedMonths = settings?.watchtowerUnusedMonths ?? 12;

  useEffect(() => {
    if (!settings) void useSettings.getState().load();
  }, [settings]);

  const setHibp = (v: boolean) => {
    useSettings
      .getState()
      .save({ hibpCheck: v })
      .catch((e) => setErr(String((e as Error)?.message ?? e)));
  };

  const setUnusedMonths = (months: number) => {
    useSettings
      .getState()
      .save({ watchtowerUnusedMonths: months })
      .catch((e) => setErr(String((e as Error)?.message ?? e)));
  };

  const [report, setReport] = useState<WatchtowerReport | null>(null);
  const [history, setHistory] = useState<ScorePoint[]>(() => loadHistory());
  const [titles, setTitles] = useState<Record<string, ItemSummary>>({});
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [ignoredOpen, setIgnoredOpen] = useState(false);

  const load = useCallback(async (h: boolean) => {
    setBusy(true);
    setErr(null);
    try {
      const [r, items] = await Promise.all([
        api.watchtower({ hibp: h }),
        api.listItems({ filter: { kind: "all" } }),
      ]);
      setReport(r);
      useBreachedIds.getState().remember(r);
      setHistory(recordReport(r));
      setTitles(Object.fromEntries(items.map((i) => [i.id, i])));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const ready = !!settings;
  useEffect(() => {
    if (ready) load(hibp);
  }, [hibp, unusedMonths, ready, load]);

  // Reload after the security checkup changes findings (#401).
  useEffect(() => {
    const onReload = () => load(hibp);
    window.addEventListener(CHECKUP_RELOAD_EVENT, onReload);
    return () => window.removeEventListener(CHECKUP_RELOAD_EVENT, onReload);
  }, [load, hibp]);

  const open = (id: string) => {
    const it = titles[id];
    setRoute({ name: "items", filter: it ? { kind: "vault", vaultId: it.vaultId } : { kind: "all" } });
    selectItem(id);
  };

  const openEdit = (id: string) => {
    const it = titles[id];
    setRoute({ name: "items", filter: it ? { kind: "vault", vaultId: it.vaultId } : { kind: "all" } });
    selectItem(id);
    setEditing(true);
  };

  const handleArchiveOlder = async (older: ItemSummary[]) => {
    try {
      await Promise.all(older.map((it) => api.archiveItem({ vaultId: it.vaultId, itemId: it.id, archived: true })));
      toast(t("bulk.toast.archive"), {
        duration: 5000,
        action: {
          label: t("bulk.undo"),
          altText: t("bulk.undo"),
          onClick: async () => {
            try {
              await Promise.all(older.map((it) => api.archiveItem({ vaultId: it.vaultId, itemId: it.id, archived: false })));
              toast(t("bulk.toast.unarchive"));
              load(hibp);
            } catch (e) {
              toast(String(e), "danger");
            }
          },
        },
      });
      load(hibp);
    } catch (e) {
      toast(String(e), "danger");
    }
  };

  const archiveUnused = async (id: string) => {
    const it = titles[id];
    if (!it) return;
    try {
      await api.archiveItem({ vaultId: it.vaultId, itemId: id, archived: true });
      toast(t("watchtower.archivedToast"), {
        duration: 5000,
        action: {
          label: t("common.undo"),
          altText: t("common.undo"),
          onClick: async () => {
            try {
              await api.archiveItem({ vaultId: it.vaultId, itemId: id, archived: false });
              toast(t("bulk.toast.unarchive"));
              load(hibp);
            } catch (e) {
              toast(String(e), "danger");
            }
          },
        },
      });
      load(hibp);
    } catch (e) {
      toast(String(e), "danger");
    }
  };

  const changePassword = (id: string) => {
    const it = titles[id];
    if (!it) return;
    api
      .openChangePassword({ vaultId: it.vaultId, itemId: id })
      .catch((e) => toast(String((e as Error)?.message ?? e), "danger"));
  };

  const ignoreFinding = async (id: string, kind: string) => {
    const it = titles[id];
    const vaultId = it?.vaultId;
    if (!vaultId) return;
    try {
      await api.ignoreFinding({ vaultId, itemId: id, kind });
      await load(hibp);
      toast(t("watchtower.ignoredToast"), {
        duration: 5000,
        action: {
          label: t("common.undo"),
          onClick: async () => {
            try {
              await api.unignoreFinding({ vaultId, itemId: id, kind });
              await load(hibp);
              toast(t("watchtower.restoredToast"));
            } catch (e) {
              toast(String((e as Error)?.message ?? e), "danger");
            }
          },
        },
      });
    } catch (e) {
      toast(String((e as Error)?.message ?? e), "danger");
    }
  };

  const stopIgnoring = async (id: string, kind: string, vaultId: string) => {
    try {
      await api.unignoreFinding({ vaultId, itemId: id, kind });
      await load(hibp);
      toast(t("watchtower.restoredToast"), {
        duration: 5000,
        action: {
          label: t("common.undo"),
          onClick: async () => {
            try {
              await api.ignoreFinding({ vaultId, itemId: id, kind });
              await load(hibp);
              toast(t("watchtower.ignoredToast"));
            } catch (e) {
              toast(String((e as Error)?.message ?? e), "danger");
            }
          },
        },
      });
    } catch (e) {
      toast(String((e as Error)?.message ?? e), "danger");
    }
  };

  return (
    <div className="h-full overflow-y-auto" data-testid="watchtower-scroll">
    <div className="mx-auto max-w-5xl space-y-6 p-8">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">{t("nav.watchtower")}</h1>
        <div className="flex items-center gap-2">
          {report && buildCheckupQueue(report).length > 0 && (
            <Button size="sm" onClick={() => useApp.getState().openModal('security-checkup')} data-testid="start-checkup">
              <ShieldCheck size={14} /> {t("watchtower.startCheckup")}
            </Button>
          )}
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => load(hibp)}>
            <RefreshCw size={14} /> {t("watchtower.recheck")}
          </Button>
        </div>
      </div>
      {err && (
        <p role="alert" className="text-sm text-danger">
          {err}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-8 rounded-xl border border-border bg-panel p-6">
        {report ? <ScoreGauge score={report.score} /> : <Skeleton className="h-40 w-40 rounded-full" />}
        <div className="max-w-md flex-1 space-y-3">
          <p className="text-fg">
            {t("watchtower.scoreExplanation")}
          </p>
          {report && (
            <p className="text-xs text-muted">
              {t(report.hibpChecked ? "watchtower.lastCheckedOn" : "watchtower.lastCheckedOff", {
                date: new Date(report.checkedAt).toLocaleString(lang),
              })}
            </p>
          )}
          <div className="rounded-lg border border-border p-3">
            <label className="flex items-center gap-2 text-sm font-medium">
              <input
                type="checkbox"
                role="switch"
                aria-label={t('watchtower.breachCheckAria')}
                checked={hibp}
                onChange={(e) => setHibp(e.target.checked)}
                className="accent-accent"
              />{" "}
              {t('watchtower.breachCheckLabel')}
            </label>
            <p className="mt-1 text-xs text-muted">
              {t("watchtower.hibpNoteIntro")}{" "}
              <b>{t("watchtower.hibpNoteFirst5")}</b>{" "}
              {t("watchtower.hibpNoteRest")}
            </p>
          </div>
          <div className="rounded-lg border border-border p-3">
            <label className="flex flex-wrap items-center gap-2 text-sm font-medium">
              <History size={14} className="text-muted" />
              {t("watchtower.unusedAfter")}
              <select
                aria-label={t("watchtower.unusedAfter")}
                className="h-8 rounded-md border border-border bg-bg px-2 text-sm text-fg focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                value={unusedMonths}
                disabled={!settings}
                onChange={(e) => setUnusedMonths(Number(e.target.value))}
                data-testid="unused-months-select"
              >
                {[6, 12, 24].map((n) => (
                  <option key={n} value={n}>
                    {t("watchtower.unusedMonths", { n })}
                  </option>
                ))}
                <option value={0}>{t("watchtower.unusedOff")}</option>
              </select>
            </label>
            <p className="mt-1 text-xs text-muted">{t("watchtower.unusedHint")}</p>
          </div>
        </div>
      </div>
      <div className="rounded-xl border border-border bg-panel p-4">
        <ScoreHistory points={history} />
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        {cards.map(({ key, title, desc, Icon }) => {
          const ids = report ? issueIds(report, key) : [];
          const offHint =
            key === "breached" && report && !report.hibpChecked
              ? t("watchtower.breachOffHint")
              : key === "unused" && report && unusedMonths === 0
              ? t("watchtower.unusedOffHint")
              : null;
          const kind = ISSUE_TO_KIND[key];
          // Change password is offered for password-related findings only (issue #92).
          const canChangePassword = key === "weak" || key === "reused" || key === "breached" || key === "rotationDue" || key === "rotationSoon";
          const rotationDays = new Map((key === "rotationDue" ? report?.rotationDue : key === "rotationSoon" ? report?.rotationSoon : undefined)?.map((r) => [r.itemId, r.days]));
          return (
            <section key={key} aria-label={title} className="rounded-xl border border-border bg-panel p-4">
              <div className="flex items-center gap-2">
                <Icon
                  size={18}
                  className={
                    ids.length
                      ? key === "expired"
                        ? "text-danger"
                        : key === "expiringSoon" || key === "rotationSoon"
                        ? "text-accent"
                        : "text-warn"
                      : "text-ok"
                  }
                />
                <h2 className="font-semibold">{title}</h2>
                <span data-testid={`count-${key}`} className="ml-auto rounded-full bg-hover px-2 text-sm">
                  {report ? (offHint ? "—" : ids.length) : "…"}
                </span>
              </div>
              <p className="mt-1 text-xs text-muted">{desc}</p>
              {offHint ? (
                <p className="mt-3 text-xs text-muted">{offHint}</p>
              ) : ids.length === 0 ? (
                report && (
                  <p className="mt-3 flex items-center gap-1 text-xs text-ok">
                    <ShieldCheck size={12} /> {t("watchtower.noIssues")}
                  </p>
                )
              ) : key === "reused" ? (
                <ul className="mt-3 space-y-2">
                  {report!.reused.map((g, i) => (
                    <li key={i} className="rounded bg-bg p-2">
                      <div className="mb-1 text-[11px] text-muted">
                        {t("watchtower.duplicateGroup", { n: i + 1, count: g.length })}
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {g.map((id) => (
                          <FindingChip
                            key={id}
                            id={id}
                            title={titles[id]?.title ?? id}
                            onOpen={() => open(id)}
                            onIgnore={() => ignoreFinding(id, "reused")}
                            onChangePassword={
                              canChangePassword && titles[id]?.changePasswordUrl ? () => changePassword(id) : undefined
                            }
                          />
                        ))}
                      </div>
                    </li>
                  ))}
                </ul>
              ) : key === "duplicates" ? (
                <ul className="mt-3 space-y-3">
                  {report!.duplicates.map((g, i) => {
                    const groupItems = g.map(
                      (id) =>
                        titles[id] ?? {
                          id,
                          vaultId: "",
                          category: "login" as const,
                          title: id,
                          subtitle: "",
                          favorite: false,
                          archived: false,
                          deletedAt: null,
                          tags: [],
                          updatedAt: "",
                          createdAt: "",
                        }
                    );
                    const sorted = [...groupItems].sort(
                      (a, b) => (Date.parse(b.updatedAt) || 0) - (Date.parse(a.updatedAt) || 0)
                    );
                    const older = sorted.slice(1);
                    const groupTitle = t("watchtower.duplicateGroup", { n: i + 1, count: g.length });
                    return (
                      <li key={i} className="rounded-lg border border-border bg-bg p-3">
                        <div className="mb-2 flex items-center justify-between gap-2">
                          <span className="text-[11px] font-medium text-muted">{groupTitle}</span>
                          {older.length > 0 && (
                            <Button
                              size="sm"
                              variant="outline"
                              className="h-7 px-2 text-xs border-accent text-accent hover:bg-accent/10 focus-visible:ring-accent"
                              onClick={() => void handleArchiveOlder(older)}
                              aria-label={t("watchtower.archiveOlderFor", { group: groupTitle })}
                            >
                              <Archive size={12} className="mr-1" />
                              {t("watchtower.archiveOlder")}
                            </Button>
                          )}
                        </div>
                        <div className="space-y-1.5">
                          {sorted.map((item) => (
                            <div
                              key={item.id}
                              className="flex items-center justify-between gap-3 rounded bg-panel px-2.5 py-1.5 text-xs"
                            >
                              <div className="min-w-0 flex-1">
                                <div className="truncate font-medium text-fg">{item.title}</div>
                                <div className="truncate text-[11px] text-muted">
                                  {item.subtitle ? `${item.subtitle} · ` : ""}
                                  {item.url
                                    ? `${item.url.replace(/^[a-zA-Z]+:\/\//, "").replace(/\/.*$/, "")} · `
                                    : ""}
                                  {item.updatedAt ? new Date(item.updatedAt).toLocaleDateString() : ""}
                                </div>
                              </div>
                              {item.changePasswordUrl && (
                                <button
                                  type="button"
                                  onClick={() => changePassword(item.id)}
                                  aria-label={t("watchtower.changePasswordFor", { title: item.title })}
                                  className="rounded px-1 py-1 text-muted hover:text-accent focus-visible:text-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                                >
                                  <ExternalLink size={12} />
                                </button>
                              )}
                              <Button
                                size="sm"
                                variant="ghost"
                                className="h-6 px-2 text-xs"
                                onClick={() => open(item.id)}
                              >
                                {t("watchtower.open")}
                              </Button>
                            </div>
                          ))}
                        </div>
                      </li>
                    );
                  })}
                </ul>
              ) : key === "insecureUrls" ? (
                <ul className="mt-3 space-y-2" data-testid="insecure-urls-list">
                  {ids.map((id) => {
                    const it = titles[id];
                    const rawUrl = it?.url ?? "";
                    const httpsUrl = rawUrl.replace(/^http:\/\//i, "https://");
                    return (
                      <li
                        key={id}
                        className="flex flex-col gap-2 rounded-lg bg-bg p-2.5 text-xs sm:flex-row sm:items-center sm:justify-between"
                        data-testid={`insecure-item-${id}`}
                      >
                        <div className="min-w-0 flex-1">
                          <button
                            type="button"
                            onClick={() => open(id)}
                            className="truncate text-left font-medium text-fg hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                          >
                            {it?.title ?? id}
                          </button>
                          {rawUrl && <div className="truncate font-mono text-[11px] text-muted">{rawUrl}</div>}
                        </div>
                        <div className="flex shrink-0 items-center gap-1.5">
                          {httpsUrl && (
                            <a
                              href={httpsUrl}
                              target="_blank"
                              rel="noreferrer noopener"
                              className="inline-flex items-center gap-1 rounded bg-hover px-2 py-1 text-[11px] font-medium text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                              aria-label={t("watchtower.openHttpsFor", { title: it?.title ?? id })}
                            >
                              <ExternalLink size={12} className="text-accent" />
                              <span>{t("watchtower.openHttps")}</span>
                            </a>
                          )}
                          {it?.changePasswordUrl && (
                            <button
                              type="button"
                              onClick={() => changePassword(id)}
                              className="inline-flex items-center gap-1 rounded bg-hover px-2 py-1 text-[11px] font-medium text-muted hover:bg-hover/80 hover:text-accent focus-visible:text-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                              aria-label={t("watchtower.changePasswordFor", { title: it.title })}
                            >
                              <ExternalLink size={12} />
                              <span>{t("watchtower.changePassword")}</span>
                            </button>
                          )}
                          <button
                            type="button"
                            onClick={() => openEdit(id)}
                            className="inline-flex items-center gap-1 rounded bg-hover px-2 py-1 text-[11px] font-medium text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                            aria-label={t("watchtower.editUrlFor", { title: it?.title ?? id })}
                          >
                            <Pencil size={12} className="text-accent" />
                            <span>{t("watchtower.editUrl")}</span>
                          </button>
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <button
                                type="button"
                                aria-label={t("watchtower.moreOptionsFor", { title: it?.title ?? id })}
                                className="inline-flex items-center justify-center rounded bg-hover p-1 text-muted hover:text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                              >
                                <MoreVertical size={13} />
                              </button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end">
                              <DropdownMenuItem onSelect={() => ignoreFinding(id, "insecure_url")}>
                                <EyeOff size={14} />
                                <span>{t("watchtower.ignoreWarning")}</span>
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              ) : key === "unused" ? (
                <ul className="mt-3 space-y-2" data-testid="unused-list">
                  {(report?.unused ?? []).map((u) => {
                    const it = titles[u.itemId];
                    const title = it?.title ?? u.itemId;
                    const months = Math.max(1, Math.round(u.days / 30));
                    return (
                      <li
                        key={u.itemId}
                        className="flex items-center gap-2 rounded-lg bg-bg p-2.5 text-xs"
                        data-testid={`unused-item-${u.itemId}`}
                      >
                        <SiteIcon item={{ title, url: it?.url }} size={18} className="shrink-0" />
                        <button
                          type="button"
                          onClick={() => open(u.itemId)}
                          className="min-w-0 flex-1 truncate text-left font-medium text-fg hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                        >
                          {title}
                        </button>
                        <span className="shrink-0 text-[11px] text-muted">
                          {t(u.lastUsed ? "watchtower.lastUsedMonthsAgo" : "watchtower.lastEditedMonthsAgo", { n: months })}
                        </span>
                        <Button
                          size="sm"
                          variant="outline"
                          className="h-7 shrink-0 border-accent px-2 text-xs text-accent hover:bg-accent/10 focus-visible:ring-accent"
                          onClick={() => void archiveUnused(u.itemId)}
                          aria-label={t("watchtower.archiveFor", { title })}
                        >
                          <Archive size={12} className="mr-1" />
                          {t("watchtower.archive")}
                        </Button>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <button
                              type="button"
                              aria-label={t("watchtower.moreOptionsFor", { title })}
                              className="inline-flex shrink-0 items-center justify-center rounded bg-hover p-1 text-muted hover:text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                            >
                              <MoreVertical size={13} />
                            </button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem onSelect={() => ignoreFinding(u.itemId, "unused")}>
                              <EyeOff size={14} />
                              <span>{t("watchtower.ignoreWarning")}</span>
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </li>
                    );
                  })}
                </ul>
              ) : (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {ids.map((id) => (
                    <FindingChip
                      key={id}
                      id={id}
                      title={titles[id]?.title ?? id}
                      reason={
                        key === "weak"
                          ? report?.weakReasons?.[id]
                          : key === "rotationDue"
                          ? t("watchtower.rotationDueReason", { n: rotationDays.get(id) ?? 0 })
                          : key === "rotationSoon"
                          ? rotationDays.get(id) === 1
                            ? t("watchtower.rotationSoonReasonOne")
                            : t("watchtower.rotationSoonReason", { n: rotationDays.get(id) ?? 0 })
                          : undefined
                      }
                      onOpen={() => open(id)}
                      onIgnore={() => ignoreFinding(id, kind)}
                      onChangePassword={
                        canChangePassword && titles[id]?.changePasswordUrl ? () => changePassword(id) : undefined
                      }
                    />
                  ))}
                </div>
              )}
            </section>
          );
        })}
      </div>

      {report && (
        <section
          className="rounded-xl border border-border bg-panel p-4"
          aria-label={t("watchtower.ignoredSection")}
          data-testid="ignored-section"
        >
          <button
            type="button"
            onClick={() => setIgnoredOpen(!ignoredOpen)}
            className="flex w-full items-center justify-between font-semibold text-fg focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
            aria-expanded={ignoredOpen}
            data-testid="toggle-ignored-section"
          >
            <div className="flex items-center gap-2">
              <ChevronRight size={16} className={cn("transition-transform text-muted", ignoredOpen && "rotate-90")} />
              <EyeOff size={16} className="text-muted" />
              <h2>{t("watchtower.ignoredSection")}</h2>
            </div>
            <span data-testid="count-ignored" className="rounded-full bg-hover px-2 text-sm text-muted">
              {report.ignored?.length ?? 0}
            </span>
          </button>
          {ignoredOpen && (
            <div className="mt-3">
              {!report.ignored || report.ignored.length === 0 ? (
                <p className="text-xs text-muted">{t("watchtower.noIgnored")}</p>
              ) : (
                <ul className="space-y-2" data-testid="ignored-findings-list">
                  {report.ignored.map((item) => (
                    <li
                      key={`${item.itemId}-${item.kind}`}
                      className="flex items-center justify-between rounded-lg bg-bg p-2.5 text-xs"
                      data-testid={`ignored-item-${item.itemId}-${item.kind}`}
                    >
                      <div className="min-w-0 flex-1">
                        <button
                          type="button"
                          onClick={() => open(item.itemId)}
                          className="truncate font-medium text-fg hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                        >
                          {item.title}
                        </button>
                        <div className="truncate text-[11px] text-muted">
                          {item.description || item.kind}
                        </div>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => stopIgnoring(item.itemId, item.kind, item.vaultId)}
                        aria-label={t("watchtower.stopIgnoringFor", { title: item.title })}
                      >
                        {t("watchtower.stopIgnoring")}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </section>
      )}
    </div>
    </div>
  );
}

function localizeWeakReason(reason: string | undefined, t: (k: any) => string): string | undefined {
  if (!reason) return undefined;
  if (reason === "contains the username") return t("watchtower.weakReason.containsUsername");
  if (reason === "contains the site name") return t("watchtower.weakReason.containsSite");
  if (reason === "contains a very common password") return t("watchtower.weakReason.common");
  if (reason === "too few distinct characters") return t("watchtower.weakReason.fewChars");
  return reason;
}

function FindingChip({
  id,
  title,
  reason,
  onOpen,
  onIgnore,
  onChangePassword,
}: {
  id: string;
  title: string;
  reason?: string;
  onOpen: () => void;
  onIgnore: () => void;
  onChangePassword?: () => void;
}) {
  const t = useT();
  const [menuOpen, setMenuOpen] = useState(false);
  const localizedReason = localizeWeakReason(reason, t);
  return (
    <div
      className="group inline-flex items-center rounded-md bg-hover text-xs focus-within:ring-1 focus-within:ring-accent"
      onKeyDown={(e) => {
        if (e.key === "F10" && e.shiftKey) {
          e.preventDefault();
          setMenuOpen(true);
        }
      }}
      data-testid={`finding-chip-${id}`}
    >
      <button
        type="button"
        onClick={onOpen}
        className="rounded-l-md px-2 py-1 text-fg hover:underline focus-visible:outline-none"
        title={localizedReason ? `${title} (${localizedReason})` : undefined}
      >
        <span>{title}</span>
        {localizedReason && (
          <span className="ml-1 text-[11px] text-muted font-normal" data-testid={`chip-reason-${id}`}>
            ({localizedReason})
          </span>
        )}
      </button>
      {onChangePassword && (
        <button
          type="button"
          onClick={onChangePassword}
          aria-label={t("watchtower.changePasswordFor", { title })}
          className="px-1 py-1 text-muted hover:text-accent focus-visible:text-accent focus-visible:outline-none"
        >
          <ExternalLink size={12} />
        </button>
      )}
      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            onClick={() => setMenuOpen((o) => !o)}
            aria-label={t("watchtower.moreOptionsFor", { title })}
            className="rounded-r-md px-1 py-1 text-muted hover:text-fg focus-visible:outline-none"
          >
            <MoreVertical size={12} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={onIgnore}>
            <EyeOff size={14} />
            <span>{t("watchtower.ignoreWarning")}</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
