import { describe, it, expect, beforeEach } from 'vitest';
import { useApp, defaultRoute } from './store';
describe('useApp store', () => {
  beforeEach(() => useApp.setState({ route: defaultRoute, selectedItemId: null, modal: null, locked: true, editing: false }));
  it('exposes contract API', () => {
    const s = useApp.getState();
    for (const k of ['route', 'setRoute', 'openModal', 'closeModal', 'selectedItemId', 'selectItem', 'locked', 'lock']) expect(k in s).toBe(true);
  });
  it('setRoute clears selection and edit mode', () => {
    useApp.getState().selectItem('a'); useApp.getState().setEditing(true);
    useApp.getState().setRoute({ name: 'watchtower' });
    expect(useApp.getState()).toMatchObject({ route: { name: 'watchtower' }, selectedItemId: null, editing: false });
  });
  it('modals open/close with props', () => {
    useApp.getState().openModal('new-item', { category: 'login' });
    expect(useApp.getState().modal).toEqual({ id: 'new-item', props: { category: 'login' } });
    useApp.getState().closeModal(); expect(useApp.getState().modal).toBeNull();
  });
  it('lock resets UI state', () => {
    useApp.getState().unlock(); useApp.getState().selectItem('x'); useApp.getState().openModal('m');
    useApp.getState().lock();
    expect(useApp.getState()).toMatchObject({ locked: true, selectedItemId: null, modal: null, route: defaultRoute });
  });
  it('theme toggles html.dark', () => {
    useApp.getState().setTheme('light'); expect(document.documentElement.classList.contains('dark')).toBe(false);
    useApp.getState().setTheme('dark'); expect(document.documentElement.classList.contains('dark')).toBe(true);
  });
});
