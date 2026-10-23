// Tabs for Tablo's `Tabs` blocks.
//
// A block renders `[data-tabs]` holding:
//
// * a `[role=tablist]` of `button[role=tab]`, each naming its panel by id in
//   `aria-controls`;
// * one `[data-tab-panel]` per tab, naming its tab by id in `aria-labelledby`.
//
// The server marks the active tab `aria-selected="true"` and hides every other
// panel. A click, or an arrow, `Home` or `End` key on a tab, moves that mark.
//
// Every panel stays in the form, so an inactive tab's controls still post and
// the browser still checks them. A control the browser refuses on submit opens
// its tab during the `invalid` event, which the browser fires before it focuses
// the first refused control: a control in a hidden panel cannot take focus, and
// the submit would stop with no message in sight.
//
// A script rather than Topcoat handlers: Topcoat binds a handler to its own
// element, and `invalid` does not bubble.

// Make `tab` the active tab of its list in `doc`, showing its panel and hiding
// the others.
function select(doc, tab) {
  const list = tab.closest('[role=tablist]');
  if (!list) return;
  for (const other of Array.from(list.querySelectorAll('[role=tab]'))) {
    const active = other === tab;
    other.setAttribute('aria-selected', String(active));
    other.setAttribute('tabindex', active ? '0' : '-1');
    const panel = doc.getElementById(other.getAttribute('aria-controls'));
    if (panel) panel.hidden = !active;
  }
}

// The tab a key press on `tab` moves to, or `null` for any other key. A tab a
// condition hides sits under a `[hidden]` fieldset, and the keys skip it.
function target(tab, key) {
  const list = tab.closest('[role=tablist]');
  if (!list) return null;
  const tabs = Array.from(list.querySelectorAll('[role=tab]')).filter(
    (candidate) => candidate === tab || !candidate.closest('[hidden]'),
  );
  const at = tabs.indexOf(tab);
  switch (key) {
    case 'ArrowRight':
      return tabs[(at + 1) % tabs.length];
    case 'ArrowLeft':
      return tabs[(at - 1 + tabs.length) % tabs.length];
    case 'Home':
      return tabs[0];
    case 'End':
      return tabs[tabs.length - 1];
    default:
      return null;
  }
}

// Open every hidden panel holding `control`, from the innermost out, so a tab
// nested in another tab shows too.
function reveal(doc, control) {
  let panel = control.closest && control.closest('[data-tab-panel]');
  while (panel) {
    if (panel.hidden) {
      const tab = doc.getElementById(panel.getAttribute('aria-labelledby'));
      if (tab) select(doc, tab);
    }
    panel = panel.parentElement && panel.parentElement.closest('[data-tab-panel]');
  }
}

// The tab of a `Tabs` block holding `node`, or `null`.
function tabOf(node) {
  const tab = node.closest && node.closest('[role=tab]');
  return tab && tab.closest('[data-tabs]') ? tab : null;
}

// Document-level delegation: runtime navigation replaces the page without
// running this script again, so a listener bound to a tab would be lost with
// the first navigation.
function install(doc, defer = (f) => setTimeout(f, 0)) {
  doc.addEventListener('click', (e) => {
    const tab = tabOf(e.target);
    if (tab) select(doc, tab);
  });
  doc.addEventListener('keydown', (e) => {
    const tab = tabOf(e.target);
    const next = tab && target(tab, e.key);
    if (!next) return;
    e.preventDefault();
    select(doc, next);
    next.focus();
  });
  // One submit fires `invalid` at every refused control in document order, then
  // focuses the first: only that one's tab opens.
  let checking = false;
  doc.addEventListener(
    'invalid',
    (e) => {
      if (checking) return;
      checking = true;
      defer(() => {
        checking = false;
      });
      reveal(doc, e.target);
    },
    true,
  );
}

if (typeof document !== 'undefined') {
  install(document);
}

// Exposed for the Node unit test (`tabs.test.js`); the guard keeps the export
// out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { install, reveal, select, target };
}
