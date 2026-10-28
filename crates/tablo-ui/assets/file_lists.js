// Ordering for Tablo file lists.
//
// A file list renders `[data-file-list]` holding one `[data-file-item]` row per
// stored file. Each row posts its path under the list's `keep_<field>` key,
// and the server keeps the rows in the order the form posts them, which is the
// order they show. A row's `[data-file-up]` and `[data-file-down]` buttons move
// it, so moving a row touches nothing but its place. Each button is
// `type="button"`, so none submits the form.

// Move the row holding `button` one place up or down; `false` when the button
// is no file list's.
function act(button) {
  const row = button.closest('[data-file-item]');
  const list = button.closest('[data-file-list]');
  if (!row || !list) return false;
  let moved = false;
  if (button.hasAttribute('data-file-up')) {
    const previous = row.previousElementSibling;
    if (previous) list.insertBefore(row, previous);
    moved = Boolean(previous);
  } else if (button.hasAttribute('data-file-down')) {
    const next = row.nextElementSibling;
    if (next) list.insertBefore(next, row);
    moved = Boolean(next);
  } else {
    return false;
  }
  button.focus();
  // A move changes what the form saves, so the leave-page guard in `forms.js`
  // counts it as an edit.
  if (moved) list.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
}

const BUTTONS = '[data-file-up], [data-file-down]';

// Everything below only makes sense with a document. It lives in a function so
// this file can also be `require`d by its Node unit test, which has no DOM.
function install() {
  document.addEventListener('click', (e) => {
    const button = e.target.closest && e.target.closest(BUTTONS);
    if (button) act(button);
  });
}

if (typeof document !== 'undefined') install();

// Exposed for the Node unit test (`file_lists.test.js`); the guard keeps the
// export out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { act };
}
