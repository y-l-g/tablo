// Focuses the global search box on Cmd+K, or Ctrl+K off macOS.
//
// The box's input declares the shortcut in `aria-keyshortcuts`, so a screen reader announces it.
// A page without the box, such as sign-in or a panel without global search, leaves the keys to the
// browser.
//
// Hooks: `[data-global-search]` holding the `input[type="search"]` the shortcut focuses.

// The search input `doc` shows, or `null` when it renders none.
function searchInput(doc) {
  const input = doc.querySelector('[data-global-search] input[type="search"]');
  if (!input || input.getClientRects().length === 0) return null;
  return input;
}

// Whether `e` is the shortcut: K with Cmd or Ctrl and no other modifier.
function isShortcut(e) {
  return (
    (e.metaKey || e.ctrlKey) &&
    !e.altKey &&
    !e.shiftKey &&
    typeof e.key === 'string' &&
    e.key.toLowerCase() === 'k'
  );
}

// Document-level delegation: runtime navigation replaces the page without running this script
// again, so a listener bound to the input would be lost with the first navigation.
function install(doc) {
  doc.addEventListener('keydown', (e) => {
    if (!isShortcut(e)) return;
    const input = searchInput(doc);
    if (!input) return;
    e.preventDefault();
    input.focus();
    input.select();
  });
}

if (typeof document !== 'undefined') {
  install(document);
}

// Exposed for the Node unit test (`search_shortcut.test.js`); the guard keeps the export out of
// the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { install, isShortcut };
}
