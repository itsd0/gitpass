const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Native save dialog in the Tauri app. The browser mock (dev/tests) uses a Downloads path; nothing is written there. */
export async function chooseExportPath(defaultName: string, ext: 'json' | 'csv'): Promise<string | null> {
  if (!isTauri()) return `~/Downloads/${defaultName}`;
  const { save } = await import('@tauri-apps/plugin-dialog');
  return save({ defaultPath: defaultName, filters: [{ name: ext === 'json' ? 'gitpass JSON' : 'CSV', extensions: [ext] }] });
}
