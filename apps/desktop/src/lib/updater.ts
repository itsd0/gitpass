/** Auto-update via tauri-plugin-updater (GitHub Releases `latest.json`). Outside Tauri (browser/mock/tests)
 * there is nothing to update, so `checkForUpdate` reports `unsupported`. */
export type UpdateCheck =
  | { kind: 'unsupported' }
  | { kind: 'none' }
  | { kind: 'available'; version: string; notes?: string; install: (onProgress?: (pct: number | null) => void) => Promise<void> };

/** App version: the bundle's real version (tauri.conf.json) inside Tauri, package.json version otherwise. */
export async function getAppVersion(): Promise<string> {
  if (!isTauri()) return __APP_VERSION__;
  try { const { getVersion } = await import('@tauri-apps/api/app'); return await getVersion(); } catch { return __APP_VERSION__; }
}

/** The updater's error when the endpoint has no release yet (404 / no latest.json): treat as "up to date". */
export function isNoReleaseError(e: unknown): boolean {
  const m = String((e as Error)?.message ?? e);
  return /valid release JSON|\b404\b|not found/i.test(m);
}

const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function checkForUpdate(): Promise<UpdateCheck> {
  if (!isTauri()) return { kind: 'unsupported' };
  const { check } = await import('@tauri-apps/plugin-updater');
  let u;
  try { u = await check(); } catch (e) { if (isNoReleaseError(e)) return { kind: 'none' }; throw e; }
  if (!u) return { kind: 'none' };
  return {
    kind: 'available', version: u.version, notes: u.body ?? undefined,
    install: async (onProgress) => {
      let total = 0, done = 0;
      await u.downloadAndInstall((e) => {
        if (e.event === 'Started') total = e.data.contentLength ?? 0;
        else if (e.event === 'Progress') { done += e.data.chunkLength; onProgress?.(total ? Math.round((done / total) * 100) : null); }
      });
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    },
  };
}
