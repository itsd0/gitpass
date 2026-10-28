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
import { api } from "../../api/client";
import { useT } from "../../i18n";
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
import { gaugeDash, issueIds, scoreBand, type IssueKey } from "./score";
import { cn } from "../../lib/utils";

export function ScoreGauge({ score }: { score: number }) {
  const b = scoreBand(score);
  const d = gaugeDash(score);
  return (
    <div
      className="relative h-40 w-40"
      role="img"
      aria-label={`Security score ${b.score} of 100, ${b.label}`}
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
          {b.label}
        </span>
      </div>
    </div>
  );
}

const CARDS: { key: IssueKey; title: string; desc: string; Icon: typeof AlertTriangle }[] = [
  { key: "weak", title: "Weak passwords", desc: "Easy to guess. Generate a strong replacement.", Icon: AlertTriangle },
  { key: "reused", title: "Reused passwords", desc: "Same password on multiple sites.", Icon: Copy },
  { key: "old", title: "Old passwords", desc: "Not changed in a long time.", Icon: Clock },
  { key: "breached", title: "Breached passwords", desc: "Found in known data breaches (HIBP).", Icon: ShieldAlert },
  { key: "missingTotp", title: "Missing 2FA", desc: "Logins with a URL but no one-time password saved.", Icon: Smartphone },
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
};

export function WatchtowerPage() {
  const t = useT();
  const cards = [
    ...CARDS,
    { key: "duplicates" as const, title: t("watchtower.duplicates"), desc: t("watchtower.duplicatesDesc"), Icon: Layers },
    { key: "totpAvailable" as const, title: t("watchtower.totpAvailable"), desc: t("watchtower.totpAvailableDesc"), Icon: Smartphone },
    { key: "passkeyAvailable" as const, title: t("watchtower.passkeyAvailable"), desc: t("watchtower.passkeyAvailableDesc"), Icon: KeyRound },
    { key: "expired" as const, title: t("watchtower.expired"), desc: t("watchtower.expiredDesc"), Icon: CalendarX },
    { key: "expiringSoon" as const, title: t("watchtower.expiringSoon"), desc: t("watchtower.expiringSoonDesc"), Icon: CalendarClock },
    { key: "insecureUrls" as const, title: t("watchtower.insecureUrls"), desc: t("watchtower.insecureUrlsDesc"), Icon: Globe },
  ];
  const { selectItem, setRoute, setEditing } = useApp();
  const settings = useSettings((st) => st.settings);
  const hibp = !!settings?.hibpCheck;

  useEffect(() => {
    if (!settings) void useSettings.getState().load();
  }, [settings]);

  const setHibp = (v: boolean) => {
    useSettings
      .getState()
      .save({ hibpCheck: v })
      .catch((e) => setErr(String((e as Error)?.message ?? e)));
  };

  const [report, setReport] = useState<WatchtowerReport | null>(null);
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
  }, [hibp, ready, load]);

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
    <div className="mx-auto max-w-5xl space-y-6 p-8">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Watchtower</h1>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => load(hibp)}>
          <RefreshCw size={14} /> Re-check
        </Button>
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
            Your security score reflects weak, reused, old and breached passwords and missing two-factor authentication.
          </p>
          {report && (
            <p className="text-xs text-muted">
              Last checked {new Date(report.checkedAt).toLocaleString()}
              {report.hibpChecked ? " · breach check included" : " · breach check off"}
            </p>
          )}
          <div className="rounded-lg border border-border p-3">
            <label className="flex items-center gap-2 text-sm font-medium">
              <input
                type="checkbox"
                role="switch"
                aria-label="Check for breached passwords"
                checked={hibp}
                onChange={(e) => setHibp(e.target.checked)}
                className="accent-accent"
              />{" "}
              Check passwords against known breaches
            </label>
            <p className="mt-1 text-xs text-muted">
              Off by default. When on, gitpass hashes each password with SHA-1 locally and sends only the{" "}
              <b>first 5 characters</b> of the hash to Have I Been Pwned (k-anonymity). HIBP returns every breached hash
              with that prefix and the match is done on your device — your passwords and full hashes never leave this
              computer, and HIBP can’t tell which password you checked.
            </p>
          </div>
        </div>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        {cards.map(({ key, title, desc, Icon }) => {
          const ids = report ? issueIds(report, key) : [];
          const off = key === "breached" && report && !report.hibpChecked;
          const kind = ISSUE_TO_KIND[key];
          // Change password is offered for password-related findings only (issue #92).
          const canChangePassword = key === "weak" || key === "reused" || key === "breached";
          return (
            <section key={key} aria-label={title} className="rounded-xl border border-border bg-panel p-4">
              <div className="flex items-center gap-2">
                <Icon
                  size={18}
                  className={
                    ids.length
                      ? key === "expired"
                        ? "text-danger"
                        : key === "expiringSoon"
                        ? "text-accent"
                        : "text-warn"
                      : "text-ok"
                  }
                />
                <h2 className="font-semibold">{title}</h2>
                <span data-testid={`count-${key}`} className="ml-auto rounded-full bg-hover px-2 text-sm">
                  {report ? (off ? "—" : ids.length) : "…"}
                </span>
              </div>
              <p className="mt-1 text-xs text-muted">{desc}</p>
              {off ? (
                <p className="mt-3 text-xs text-muted">Enable the breach check above to scan.</p>
              ) : ids.length === 0 ? (
                report && (
                  <p className="mt-3 flex items-center gap-1 text-xs text-ok">
                    <ShieldCheck size={12} /> No issues
                  </p>
                )
              ) : key === "reused" ? (
                <ul className="mt-3 space-y-2">
                  {report!.reused.map((g, i) => (
                    <li key={i} className="rounded bg-bg p-2">
                      <div className="mb-1 text-[11px] text-muted">
                        Group {i + 1} · {g.length} items
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
                              aria-label={`${t("watchtower.archiveOlder")}: ${groupTitle}`}
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
                                  aria-label={`${t("watchtower.changePassword")} for ${item.title}`}
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
                              aria-label={`${t("watchtower.openHttps")} for ${it?.title ?? id}`}
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
                              aria-label={`${t("watchtower.changePassword")} for ${it.title}`}
                            >
                              <ExternalLink size={12} />
                              <span>{t("watchtower.changePassword")}</span>
                            </button>
                          )}
                          <button
                            type="button"
                            onClick={() => openEdit(id)}
                            className="inline-flex items-center gap-1 rounded bg-hover px-2 py-1 text-[11px] font-medium text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                            aria-label={`${t("watchtower.editUrl")} for ${it?.title ?? id}`}
                          >
                            <Pencil size={12} className="text-accent" />
                            <span>{t("watchtower.editUrl")}</span>
                          </button>
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <button
                                type="button"
                                aria-label={`${t("watchtower.moreOptions")} for ${it?.title ?? id}`}
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
              ) : (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {ids.map((id) => (
                    <FindingChip
                      key={id}
                      id={id}
                      title={titles[id]?.title ?? id}
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
                        aria-label={`${t("watchtower.stopIgnoring")} for ${item.title}`}
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
  );
}

function FindingChip({
  id,
  title,
  onOpen,
  onIgnore,
  onChangePassword,
}: {
  id: string;
  title: string;
  onOpen: () => void;
  onIgnore: () => void;
  onChangePassword?: () => void;
}) {
  const t = useT();
  const [menuOpen, setMenuOpen] = useState(false);
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
      >
        {title}
      </button>
      {onChangePassword && (
        <button
          type="button"
          onClick={onChangePassword}
          aria-label={`${t("watchtower.changePassword")} for ${title}`}
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
            aria-label={`${t("watchtower.moreOptions")} for ${title}`}
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
