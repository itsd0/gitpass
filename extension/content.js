// gitpass content script (v0.6, workstream 4): field badges, inline dropdown, keyboard nav, fill,
// password suggestions, TOTP, card/identity fill, and the save/update banner. All UI in closed shadow DOM.
// Security: top frame only; https/loopback only; every fill needs a trusted (isTrusted) click or key press;
// the background derives the origin from the sender and we re-check it against location.origin before
// writing; secret values are only held in local variables for the duration of the fill.
//
// Split (issue #335): the old single IIFE now lives in content/*.js, loaded in manifest order.
// Classic content scripts share one isolated-world global scope, so the top-level declarations
// below and in content/*.js behave exactly as they did inside the IIFE. This file holds the
// shared preamble. The old IIFE guards become a flag: only a top frame on a fillable origin
// sets __gitpassInpage, and the files with side effects (settings, banner, prompts, wiring)
// check it before acting. A second injection fails at let/const redeclaration before any
// statement runs, so it cannot double-register listeners or reset state.
/* global chrome, GitpassPolicy, GitpassForms, GitpassI18n, GitpassIcons */
'use strict';
// Top frame only (no iframes, even same-origin); https or http://localhost only; once only.
if (window.top === window && GitpassPolicy.isFillableOrigin(location.origin) && !window.__gitpassInpage) window.__gitpassInpage = true;

const LOGO = 'data:image/png;base64,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';
const t = (k, f, subs) => GitpassI18n.t(k, f, subs);
const LOGIN_ROLES = new Set(['username', 'password', 'current_password', 'totp']);
const NEWPW_ROLES = new Set(['new_password', 'confirm_password']);

// Dark is the fallback palette; prefers-color-scheme:light swaps the tokens. The orange accent
// (#F28C28) stays constant for the primary button, active row indicator and badge ring.
const CSS = `:host{all:initial;position:absolute;z-index:2147483646;color-scheme:light dark;
  --gp-accent:#F28C28;--gp-accent-fg:#1C1C1E;--gp-accent-text:#F28C28;--gp-bg:#1C1C1E;--gp-fg:#ECEDF0;--gp-muted:#969AA4;--gp-border:#3c3e44;--gp-hover:#34363c;--gp-active:#45321f;--gp-field:#25262a;--gp-btn-bg:#2a2b30;--gp-overlay:rgba(0,0,0,.6);--gp-danger:#ff8a80;--gp-ok:#7ee2a8;--gp-shadow:0 12px 32px rgba(0,0,0,.45);--gp-shadow-lg:0 16px 48px rgba(0,0,0,.6);--gp-badge-shadow:0 1px 3px rgba(0,0,0,.35),0 0 0 1px rgba(242,140,40,.45);--gp-badge-hover-shadow:0 2px 6px rgba(0,0,0,.4),0 0 0 2px var(--gp-accent)}
  @media (prefers-color-scheme:light){:host{--gp-accent-text:#B25B00;--gp-bg:#FFFFFF;--gp-fg:#1C1C1E;--gp-muted:#5A5F67;--gp-border:#D5D8DE;--gp-hover:#F1F2F4;--gp-active:#FDEBD9;--gp-field:#F4F5F7;--gp-btn-bg:#EDEFF2;--gp-overlay:rgba(15,18,24,.35);--gp-danger:#B3261E;--gp-ok:#1B7F4B;--gp-shadow:0 12px 32px rgba(15,18,24,.18);--gp-shadow-lg:0 16px 48px rgba(15,18,24,.24);--gp-badge-shadow:0 1px 3px rgba(15,18,24,.22),0 0 0 1px rgba(242,140,40,.55);--gp-badge-hover-shadow:0 2px 6px rgba(15,18,24,.28),0 0 0 2px var(--gp-accent)}}
  *{box-sizing:border-box}
  .badge{all:unset;box-sizing:border-box;width:22px;height:22px;border-radius:6px;cursor:pointer;display:flex;align-items:center;justify-content:center;background:var(--gp-bg);box-shadow:var(--gp-badge-shadow);transition:transform .12s,box-shadow .12s}
  .badge:hover{transform:scale(1.08);box-shadow:var(--gp-badge-hover-shadow)}
  .badge img{width:16px;height:16px;border-radius:4px;display:block}
  .menu{width:320px;max-height:340px;overflow:auto;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:10px;box-shadow:var(--gp-shadow);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI","Noto Sans Myanmar","Noto Sans Thai",sans-serif}
  .hd{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid var(--gp-border);font-size:11px;font-weight:600;letter-spacing:.5px;text-transform:uppercase;color:var(--gp-muted)}
  .hd img{width:16px;height:16px;border-radius:4px}
  .it{display:flex;align-items:center;gap:10px;width:100%;padding:8px 10px;cursor:pointer;border:0;background:none;color:inherit;font:inherit;text-align:left}
  .site-icon-img{width:20px;height:20px;border-radius:4px;object-fit:contain;display:block;flex-shrink:0}
  .monogram{width:20px;height:20px;border-radius:50%;background:rgba(242,140,40,.16);border:1px solid rgba(242,140,40,.3);color:var(--gp-accent-text,#F28C28);font-weight:600;font-size:11px;line-height:1;display:inline-flex;align-items:center;justify-content:center;user-select:none;flex-shrink:0}
  .it:hover{background:var(--gp-hover)}
  .it.active,.it.active:hover{background:var(--gp-active);box-shadow:inset 3px 0 0 var(--gp-accent)}
  .it.primary .ti{color:var(--gp-accent-text)}
  .ic{width:28px;height:28px;flex-shrink:0;display:flex;align-items:center;justify-content:center;font-size:16px}
  .m{min-width:0;flex:1}.ti{font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.u{font-size:12px;color:var(--gp-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .mono{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
  .msg{padding:12px 10px;color:var(--gp-muted)}.err{color:var(--gp-danger)}
  .banner{position:fixed;top:16px;right:16px;width:340px;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:12px;box-shadow:var(--gp-shadow-lg);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI","Noto Sans Myanmar","Noto Sans Thai",sans-serif;padding:14px}
  .banner .row{display:flex;align-items:center;gap:10px}
  .banner .fields{display:flex;flex-direction:column;gap:8px;margin-top:12px}
  .banner .fld{display:flex;flex-direction:column;gap:4px;font-size:11px;font-weight:600;letter-spacing:.4px;text-transform:uppercase;color:var(--gp-muted)}
  .banner .inp{width:100%;box-sizing:border-box;background:var(--gp-field);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:8px;padding:7px 9px;font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI","Noto Sans Myanmar","Noto Sans Thai",sans-serif}
  .banner .inp:focus-visible,.banner .btn:focus-visible{outline:2px solid var(--gp-accent);outline-offset:2px}
  .banner .err{color:var(--gp-danger);font-size:12px;margin-top:8px}
  .banner .err[hidden]{display:none}
  .banner .acts{display:flex;align-items:center;justify-content:flex-end;gap:8px;margin-top:12px;flex-wrap:wrap}
  .banner .grow{flex:1}
  .banner .btn:disabled{opacity:.65;cursor:default}
  .banner .spin{display:inline-block;width:11px;height:11px;margin-right:6px;vertical-align:-1px;border-radius:50%;border:2px solid currentColor;border-top-color:transparent;animation:gp-spin .8s linear infinite}
  @keyframes gp-spin{to{transform:rotate(360deg)}}
  .banner .ok{color:var(--gp-ok);font-size:15px;font-weight:700}
  .btn{border:1px solid var(--gp-border);background:var(--gp-btn-bg);color:var(--gp-fg);border-radius:8px;padding:6px 12px;font:inherit;cursor:pointer}
  .btn.pri{background:var(--gp-accent);border-color:var(--gp-accent);color:var(--gp-accent-fg);font-weight:600}
  .btn.ter{border:0;background:transparent;color:var(--gp-muted);font-size:12px;padding:6px 8px;cursor:pointer}
  .btn.ter:hover{color:var(--gp-fg);text-decoration:underline}
  .tc{display:flex;align-items:stretch;width:300px;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:10px;box-shadow:var(--gp-shadow);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI","Noto Sans Myanmar","Noto Sans Thai",sans-serif;overflow:hidden}
  .tc-fill{all:unset;box-sizing:border-box;flex:1;min-width:0;display:flex;align-items:center;gap:10px;padding:8px 10px;cursor:pointer}
  .tc-fill:hover{background:var(--gp-hover)}
  .tc-fill img{width:20px;height:20px;border-radius:5px;flex-shrink:0}
  .tc .ti{display:block;font-weight:600;color:var(--gp-accent-text);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .tc .u{display:block;font-size:12px;color:var(--gp-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .tc-ring{width:20px;height:20px;flex-shrink:0;transform:rotate(-90deg)}
  .tc-track{stroke:var(--gp-border)}
  .tc-arc{stroke:var(--gp-accent);stroke-dashoffset:0;animation:gp-ring linear forwards}
  @keyframes gp-ring{to{stroke-dashoffset:50.27}}
  .tc-x{all:unset;box-sizing:border-box;width:32px;display:flex;align-items:center;justify-content:center;cursor:pointer;color:var(--gp-muted);font-size:16px;border-left:1px solid var(--gp-border)}
  .tc-x:hover{background:var(--gp-hover);color:var(--gp-fg)}
  .tc-fill:focus-visible,.tc-x:focus-visible{outline:2px solid var(--gp-accent);outline-offset:-2px}
  @media (prefers-reduced-motion:reduce){.tc-arc{animation:none}}
  .pk-overlay{position:fixed;inset:0;background:var(--gp-overlay);display:flex;align-items:center;justify-content:center;z-index:2147483647;backdrop-filter:blur(2px)}
  .pk-card{width:360px;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:12px;box-shadow:var(--gp-shadow-lg);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;padding:20px}
  .pk-row{display:flex;align-items:center;gap:12px;margin-bottom:14px}
  .pk-icon{width:32px;height:32px;border-radius:8px;display:flex;align-items:center;justify-content:center;background:var(--gp-btn-bg);flex-shrink:0}
  .pk-title{font-size:15px;font-weight:600;color:var(--gp-fg)}
  .pk-meta{background:var(--gp-field);border-radius:8px;padding:12px;margin:12px 0;display:flex;flex-direction:column;gap:8px}
  .pk-field{display:flex;justify-content:space-between;align-items:center;font-size:12px}
  .pk-label{color:var(--gp-muted)}
  .pk-val{font-weight:600;color:var(--gp-fg);max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .pk-list-header{display:flex;justify-content:space-between;align-items:center;margin-top:10px;margin-bottom:6px;font-size:12px;font-weight:600;color:var(--gp-fg)}
  .pk-list-count{font-size:11px;font-weight:normal;color:var(--gp-muted)}
  .pk-list{list-style:none;margin:0 0 10px 0;padding:0;max-height:220px;overflow-y:auto;border:1px solid var(--gp-border);border-radius:8px;background:var(--gp-field)}
  .pk-item{display:flex;align-items:center;gap:10px;padding:8px 10px;cursor:pointer;border-bottom:1px solid var(--gp-border);outline:none;user-select:none;transition:background .1s}
  .pk-item:last-child{border-bottom:none}
  .pk-item:hover{background:var(--gp-hover)}
  .pk-item.selected{background:var(--gp-active);box-shadow:inset 3px 0 0 var(--gp-accent)}
  .pk-item:focus-visible{outline:2px solid var(--gp-accent);outline-offset:-2px}
  .pk-item-info{min-width:0;flex:1;display:flex;flex-direction:column;gap:1px}
  .pk-item-user{font-weight:600;color:var(--gp-fg);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .pk-item.selected .pk-item-user{color:var(--gp-accent-text)}
  .pk-item-vault{font-size:11px;color:var(--gp-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .pk-acts{display:flex;justify-content:flex-end;gap:10px;margin-top:16px;flex-wrap:wrap}
  .pk-status{font-size:12px;margin-top:4px;color:var(--gp-muted);white-space:pre-wrap;word-break:break-word}
  .pk-status.err{color:var(--gp-danger)}
  .pk-status.ok{color:var(--gp-ok)}
  .search-wrap{display:flex;align-items:center;gap:6px;width:100%}
  .search-input{all:unset;box-sizing:border-box;width:100%;padding:4px 8px;background:var(--gp-field);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:6px;font:inherit;text-transform:none;letter-spacing:normal}
  .search-input:focus{border-color:var(--gp-accent);box-shadow:0 0 0 1px var(--gp-accent)}
  .confirm-box{padding:12px 10px;display:flex;flex-direction:column;gap:10px}
  .confirm-msg{font-size:12px;line-height:1.4;color:var(--gp-fg);word-break:break-word}
  .confirm-acts{display:flex;justify-content:flex-end;gap:8px}
  .spinner{width:16px;height:16px;border:2px solid var(--gp-border);border-top-color:var(--gp-accent);border-radius:50%;animation:spin .8s linear infinite;display:inline-block}
  @keyframes spin{to{transform:rotate(360deg)}}`;
