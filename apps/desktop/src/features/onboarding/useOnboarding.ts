import { useCallback, useEffect, useState } from 'react';
import type { Onboarding } from '../../api/types';
import { api } from '../../api/client';
// Small module-level pub/sub so badge + checklist stay in sync without touching the shell store.
const subs = new Set<(o: Onboarding) => void>();
export function useOnboarding() {
  const [o, setO] = useState<Onboarding | null>(null);
  useEffect(() => { subs.add(setO); api.onboarding().then(setO).catch(() => {}); return () => { subs.delete(setO); }; }, []);
  const patch = useCallback(async (p: Partial<Onboarding>) => { const n = await api.setOnboarding({ patch: p }); subs.forEach(f => f(n)); return n; }, []);
  return { onboarding: o, patch };
}
