'use strict';
// ---------- list row quick actions (issue #255) ----------
// Copy actions reuse copyUsername()/copyPrimary(), so concealed values and
// the 30 s clipboard auto-clear behave exactly as today, and no value is
// fetched until the button is clicked (select() loads the detail, then the
// copy path reveals only that one field). Copy number (cards) and Copy
// credential (API credentials) are both the primary-secret path.
const QUICK_ACTION_META = {
  copyUsername: ['copyUsername', 'Copy username', 'copy'],
  copyPassword: ['copyPassword', 'Copy password', 'copy'],
  copyNumber: ['copyNumber', 'Copy number', 'copy'],
  copyCredential: ['copyCredential', 'Copy credential', 'copy'],
  openWebsite: ['openWebsite', 'Open website', 'external'],
};

async function runQuickAction(action) {
  if (action === 'copyUsername') await copyUsername();
  else if (action === 'openWebsite') openWebsite();
  else await copyPrimary();
}

function quickActionsEl(it, idx, isSel, actions) {
  const wrap = el('span', 'quick-actions');
  for (const action of actions) {
    const [key, fallback, icon] = QUICK_ACTION_META[action] || [action, action, 'copy'];
    const label = t(key, fallback);
    const b = el('button', 'quick-btn');
    b.type = 'button';
    b.dataset.action = action;
    b.setAttribute('aria-label', label);
    b.title = label;
    b.tabIndex = isSel ? 0 : -1;
    b.innerHTML = UI[icon] || UI.copy;
    b.onclick = async (e) => {
      e.stopPropagation();
      try {
        // The copy paths work off the selected row + its detail, so select
        // first when this row is not the selected one, then run the action.
        if (S.sel !== idx) await select(idx);
        await runQuickAction(action);
      } catch (err) { showErr(err); }
      // select() re-rendered the row; put focus back on this button so a
      // keyboard user stays in the quick actions.
      try {
        const row = document.querySelector('#row-' + idx);
        const btn = row && row.querySelector('.quick-btn[data-action="' + action + '"]');
        if (btn) btn.focus();
      } catch (_) {}
    };
    b.ondblclick = (e) => e.stopPropagation();
    wrap.append(b);
  }
  return wrap;
}
