// The keyboard save, the unsaved-changes guard and the folded sections of
// Tablo's form pages.
//
// A form page renders `form[data-form-guard]`, the attribute holding the
// question the guard asks; the server adds `data-form-dirty` when it renders a
// refused save, whose values the user typed. A collapsible `Section` renders
// `details[data-section]`.
//
// * `Ctrl+S`, or `Cmd+S`, submits the guarded form holding the focus, else the
//   page's one guarded form, through `requestSubmit` so the browser's checks and the
//   `submit` event run as for a click on the first submit button.
// * Typing into a guarded form marks it changed. Leaving the page while one is
//   changed asks first: a followed link through `confirm`, a reload or a typed
//   address through the browser's own `beforeunload` prompt. Submitting the
//   form clears the mark, so the save itself never asks.
// * A control the browser refuses on submit opens the folded sections holding
//   it during the `invalid` event, as `tabs.js` opens its tab: a control in a
//   closed `details` cannot take focus.
//
// Without the script the form posts as it renders and the page leaves without
// asking. The history buttons leave without asking too: the runtime moves
// between pages without a `beforeunload`, and cannot be stopped.
//
// Document-level delegation: runtime navigation replaces the page without
// running this script again, so a listener bound to a form would be lost with
// the first navigation.

// The guarded form holding `node`, or `null`.
function guarded(node) {
  const form = node && node.closest && node.closest('form');
  return form && form.hasAttribute('data-form-guard') ? form : null;
}

// The changed guarded form of `doc`, or `null`.
function changed(doc) {
  return doc.querySelector('form[data-form-guard][data-form-dirty]');
}

// The form `Ctrl+S` on `target` saves: the guarded form holding it, else the
// page's only guarded form.
function saving(doc, target) {
  const own = guarded(target);
  if (own) return own;
  const forms = doc.querySelectorAll('form[data-form-guard]');
  return forms.length === 1 ? forms[0] : null;
}

// Whether `e` is the save shortcut.
function isSave(e) {
  return (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.key === 's' || e.key === 'S');
}

// Whether a click `e` follows a link away from the page in this tab: a plain
// primary click on an `a[href]` the page does not open elsewhere.
function leaves(e) {
  if (e.defaultPrevented || e.button !== 0) return null;
  if (e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return null;
  const link = e.target && e.target.closest && e.target.closest('a[href]');
  if (!link || link.hasAttribute('download')) return null;
  const target = link.getAttribute('target');
  if (target && target !== '_self') return null;
  const href = link.getAttribute('href');
  return href && !href.startsWith('#') ? link : null;
}

// Open every folded section holding `control`, from the innermost out.
function reveal(control) {
  let section = control.closest && control.closest('details[data-section]');
  while (section) {
    section.open = true;
    section = section.parentElement && section.parentElement.closest('details[data-section]');
  }
}

function install(doc, win, ask = (question) => win.confirm(question)) {
  const mark = (e) => {
    const form = guarded(e.target);
    if (form) form.setAttribute('data-form-dirty', '');
  };
  doc.addEventListener('input', mark);
  doc.addEventListener('change', mark);
  doc.addEventListener('submit', (e) => {
    const form = guarded(e.target);
    if (form) form.removeAttribute('data-form-dirty');
  });
  doc.addEventListener('keydown', (e) => {
    if (!isSave(e)) return;
    const form = saving(doc, e.target);
    if (!form) return;
    e.preventDefault();
    form.requestSubmit();
  });
  // Capture, so the guard answers before the runtime takes the click over.
  doc.addEventListener(
    'click',
    (e) => {
      const form = leaves(e) && changed(doc);
      if (!form) return;
      if (ask(form.getAttribute('data-form-guard'))) {
        form.removeAttribute('data-form-dirty');
      } else {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );
  win.addEventListener('beforeunload', (e) => {
    if (!changed(doc)) return;
    e.preventDefault();
    e.returnValue = '';
  });
  doc.addEventListener('invalid', (e) => reveal(e.target), true);
}

if (typeof document !== 'undefined') {
  install(document, window);
}

// Exposed for the Node unit test (`forms.test.js`); the guard keeps the export
// out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { install, reveal };
}
