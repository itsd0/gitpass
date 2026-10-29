const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Native open dialog for a `.gitpass-share` file in the Tauri app. Returns null outside Tauri or when the user cancels. */
export async function pickShareFile(): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const selected = await open({ multiple: false, filters: [{ name: 'gitpass share', extensions: ['gitpass-share'] }] });
  return typeof selected === 'string' ? selected : null;
}

/** Native save dialog in the Tauri app. The browser mock (dev/tests) uses a Downloads path; nothing is written there. */
export async function chooseExportPath(defaultName: string, ext: 'json' | 'csv'): Promise<string | null> {
  if (!isTauri()) return `~/Downloads/${defaultName}`;
  const { save } = await import('@tauri-apps/plugin-dialog');
  return save({ defaultPath: defaultName, filters: [{ name: ext === 'json' ? 'gitpass JSON' : 'CSV', extensions: [ext] }] });
}
