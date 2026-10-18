// Shows or masks a password while its toggle is pressed.
//
// The toggle names its input by id in `aria-controls`. Pressing it flips the input between
// `type="password"` and `type="text"`, and the toggle's `aria-pressed` and `aria-label` follow, so
// a screen reader hears what the next press does. The value never leaves the input: a form posts
// it the same either way.
//
// Hook: `[data-password-toggle]` (click).

// Flip the input `toggle` controls in `doc`; answers whether the password now shows.
function reveal(doc, toggle) {
  const input = doc.getElementById(toggle.getAttribute('aria-controls'));
  if (!input) return false;
  const shown = input.type === 'password';
  input.type = shown ? 'text' : 'password';
  toggle.setAttribute('aria-pressed', String(shown));
  toggle.setAttribute('aria-label', shown ? 'Hide password' : 'Show password');
  return shown;
}

// Document-level delegation: runtime navigation replaces the page without running this script
// again, so a listener bound to a toggle would be lost with the first navigation.
function install(doc) {
  doc.addEventListener('click', (e) => {
    const toggle = e.target.closest && e.target.closest('[data-password-toggle]');
    if (toggle) reveal(doc, toggle);
  });
}

if (typeof document !== 'undefined') {
  install(document);
}

// Exposed for the Node unit test (`password.test.js`); the guard keeps the export out of the
// browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { install, reveal };
}
