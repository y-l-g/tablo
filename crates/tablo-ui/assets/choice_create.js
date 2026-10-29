// Creating a related record from a creatable choice.
//
// A `ChoiceField::creatable()` renders `a[data-create-option]` beside its
// select, linking the related resource's create page and naming the closed
// `dialog[data-create-option-dialog]` that holds an empty
// `iframe[data-create-option-frame]`; the dialog names the select in
// `data-create-option-select`.
//
// * A plain click on the link opens the dialog and loads the create page in
//   the frame with `?pick=1`, which renders it without the panel's shell. The
//   form behind the dialog keeps what was typed in it.
// * Each page the frame loads is read: a `[data-picked]` element, the page a
//   save lands on, carries the record's key in `data-picked-value` and its
//   title in `data-picked-label`. The record becomes the select's option and
//   its value, the select announces a `change` so a condition or a searchable
//   combobox follows, and the dialog closes. A `[data-picked-hidden]` page,
//   for a record the user may not choose, leaves the choice as it was.
// * A `[data-pick-cancel]` button in the frame, Escape there that no control in
//   it took, or the dialog's own close button closes the dialog. Closing drops
//   the frame's unsaved-changes mark, so leaving it does not ask, and empties
//   it, so the next opening starts blank.
//
// Without the script the link opens the create page in a new tab.
//
// Document-level delegation, so markup a runtime navigation morphs in needs no
// re-installation. `load` and `close` do not bubble, so they are caught in the
// capture phase.

// The create page `href` in pick mode.
function pickUrl(href, base) {
  const url = new URL(href, base);
  url.searchParams.set('pick', '1');
  return url.toString();
}

// Make the record `value`, titled `label`, the choice of `select`, and
// announce it.
function choose(select, value, label) {
  let option = Array.from(select.options).find((opt) => opt.value === value);
  if (!option) {
    option = select.ownerDocument.createElement('option');
    option.value = value;
    option.textContent = label;
    select.appendChild(option);
  }
  select.value = value;
  const wrap = select.closest && select.closest('[data-select-filterable]');
  const filter = wrap && wrap.querySelector('[data-options-filter]');
  if (filter) filter.value = label;
  select.dispatchEvent(new Event('change', { bubbles: true }));
}

// Read the page the frame of `dialog` loaded, `page`: a saved record becomes
// the choice and closes the dialog. Answers whether it did.
function landed(doc, dialog, page) {
  const picked = page && page.querySelector('[data-picked]');
  if (!picked) return false;
  const select = doc.getElementById(dialog.getAttribute('data-create-option-select'));
  if (select) {
    choose(select, picked.getAttribute('data-picked-value'), picked.getAttribute('data-picked-label'));
  }
  dialog.close();
  if (select) focusChoice(select);
  return true;
}

// Focus the control the user sees for `select`: a searchable combobox's
// filter, else the select itself.
function focusChoice(select) {
  const wrap = select.closest && select.closest('[data-select-filterable]');
  const filter = wrap && wrap.querySelector('[data-options-filter]');
  (filter || select).focus();
}

// Whether a click `e` is a plain primary click, which opens the dialog; any
// other opens the link as the browser would.
function plainClick(e) {
  return e.button === 0 && !(e.ctrlKey || e.metaKey || e.shiftKey || e.altKey);
}

function install(doc) {
  doc.addEventListener('click', (e) => {
    const link = e.target.closest && e.target.closest('a[data-create-option]');
    if (!link || !plainClick(e)) return;
    const dialog = doc.getElementById(link.getAttribute('data-create-option'));
    const frame = dialog && dialog.querySelector('[data-create-option-frame]');
    if (!frame) return;
    e.preventDefault();
    frame.src = pickUrl(link.getAttribute('href'), doc.baseURI);
    dialog.showModal();
  });
  doc.addEventListener(
    'load',
    (e) => {
      const frame = e.target;
      if (!frame.matches || !frame.matches('[data-create-option-frame]')) return;
      const dialog = frame.closest('[data-create-option-dialog]');
      const page = frame.contentDocument;
      if (!dialog || !page || landed(doc, dialog, page)) return;
      page.addEventListener('click', (click) => {
        if (click.target.closest && click.target.closest('[data-pick-cancel]')) dialog.close();
      });
      page.addEventListener('keydown', (key) => {
        if (key.key === 'Escape' && !key.defaultPrevented) dialog.close();
      });
    },
    true,
  );
  doc.addEventListener(
    'close',
    (e) => {
      const dialog = e.target;
      if (!dialog.matches || !dialog.matches('[data-create-option-dialog]')) return;
      const frame = dialog.querySelector('[data-create-option-frame]');
      if (!frame) return;
      const page = frame.contentDocument;
      if (page) {
        page.querySelectorAll('form[data-form-dirty]').forEach((form) => form.removeAttribute('data-form-dirty'));
      }
      frame.src = 'about:blank';
    },
    true,
  );
}

if (typeof document !== 'undefined') {
  install(document);
}

// Exposed for the Node unit test (`choice_create.test.js`); the guard keeps the
// export out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { install, pickUrl, choose, landed };
}
