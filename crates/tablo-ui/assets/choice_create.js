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
// * The frame fits the height of the `[data-pick-page]` block of the page it
//   shows, following that block and any searchable choice's open list, so the
//   dialog is as tall as the form. Past the height of the screen the panel
//   shrinks the frame and the page scrolls inside it. Any other page, such as
//   a sign-in page, keeps the frame's own height. The form's first control
//   takes the focus, or its first invalid one after a refused save.
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

// The height the framed `page` needs, or null when it is not a pick page. An
// open searchable choice's list is positioned out of the flow, so it counts on
// its own.
function pageHeight(page) {
  const block = page.querySelector('[data-pick-page]');
  if (!block) return null;
  const top = block.getBoundingClientRect().top;
  const lists = Array.from(page.querySelectorAll('[data-options-list]')).filter((list) => !list.hidden);
  const bottoms = [block, ...lists].map((node) => node.getBoundingClientRect().bottom);
  return Math.ceil(Math.max(...bottoms) - top);
}

// Fit `frame` to `page`, twice: a frame too short for its page shows a
// scrollbar, whose width can stack a grid, so the height measured then is
// taller than the page needs once the frame fits.
function fit(frame, page) {
  for (let pass = 0; pass < 2; pass += 1) {
    const height = pageHeight(page);
    frame.style.height = height === null ? '' : `${height}px`;
  }
}

// The control a framed form starts on: its first invalid one, else its first.
function firstControl(page) {
  const controls = 'input:not([type="hidden"]), select:not([hidden]), textarea';
  return page.querySelector('form [aria-invalid="true"]') || page.querySelector(`form :is(${controls})`);
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
      // Closing empties the frame, whose blank page loads too.
      if (!dialog.open) return;
      fit(frame, page);
      frame.setAttribute('data-loaded', '');
      const control = firstControl(page);
      if (control) control.focus();
      const refit = () => fit(frame, page);
      const block = page.querySelector('[data-pick-page]');
      if (block) {
        new page.defaultView.ResizeObserver(refit).observe(block);
        new page.defaultView.MutationObserver(refit).observe(block, {
          subtree: true,
          attributes: true,
          attributeFilter: ['hidden'],
        });
      }
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
      frame.removeAttribute('data-loaded');
      frame.style.height = '';
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
  module.exports = { install, pickUrl, choose, landed, pageHeight, firstControl };
}
