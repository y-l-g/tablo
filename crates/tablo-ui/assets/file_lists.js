// Ordering, dropping and previews for Tablo file fields.
//
// A file list renders `[data-file-list]` holding one `[data-file-item]` row per
// stored file. Each row posts its path under the list's `keep_<field>` key,
// and the server keeps the rows in the order the form posts them, which is the
// order they show. A row's `[data-file-up]` and `[data-file-down]` buttons move
// it, and so does dragging its `[data-file-handle]`, so moving a row touches
// nothing but its place. Each button is `type="button"`, so none submits the
// form.
//
// Every file field's input sits in a `[data-file-drop]` zone. Files dropped on
// the zone join the input's, a list's after the ones it holds and a single
// field's in place of its one. The zone's `[data-file-pending]` lists the files
// the input holds, each a copy of the zone's `[data-file-picked-template]`
// with its name, a thumbnail when the zone is `[data-file-image]` and the file
// an image, and a `[data-file-unpick]` button taking it back out. The input is
// what posts, so the server sees nothing of this but the files it sends, in the
// order the input holds them.

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
  if (moved) changed(list);
  return true;
}

function changed(node) {
  node.dispatchEvent(new Event('change', { bubbles: true }));
}

// Put `row` of `list` before the first other row whose middle lies below `y`,
// or last; `true` when that moves it.
function place(list, row, y) {
  const others = Array.from(list.children).filter((other) => other !== row);
  const below = others.find((other) => {
    const box = other.getBoundingClientRect();
    return y < box.top + box.height / 2;
  });
  const before = below || null;
  if (row.nextElementSibling === before) return false;
  list.insertBefore(row, before);
  return true;
}

// The files a drop of `dropped` leaves `input` holding: a list's after its own,
// a single field's first dropped one alone.
function joined(input, dropped) {
  const files = Array.from(dropped);
  if (!input.multiple) return files.slice(0, 1);
  return Array.from(input.files || []).concat(files);
}

// Make `input` hold `files`, in order, and say it changed. `transfer` builds
// the `DataTransfer` the browser takes a `FileList` from.
function hold(input, files, transfer) {
  const list = transfer();
  files.forEach((file) => list.items.add(file));
  input.files = list.files;
  changed(input);
}

// The file input of the drop zone `zone`.
function inputOf(zone) {
  return zone.querySelector('input[type="file"]');
}

// List the files `input` holds in its zone `zone`, each a copy of the zone's
// template. `url` makes an image file's thumbnail address.
function render(zone, input, url) {
  const pending = zone.querySelector('[data-file-pending]');
  const template = zone.querySelector('[data-file-picked-template]');
  if (!pending || !template) return;
  pending.querySelectorAll('img[src]').forEach((img) => url.revoke(img.src));
  pending.replaceChildren();
  const image = zone.hasAttribute('data-file-image');
  Array.from(input.files || []).forEach((file, index) => {
    const tile = template.content.firstElementChild.cloneNode(true);
    const name = tile.querySelector('[data-file-picked-name]');
    name.textContent = file.name;
    name.id = `${input.id}-picked-${index}`;
    const unpick = tile.querySelector('[data-file-unpick]');
    unpick.setAttribute('data-file-unpick', String(index));
    unpick.setAttribute('aria-describedby', name.id);
    const thumb = tile.querySelector('[data-file-picked-thumb]');
    if (thumb && image && (file.type || '').startsWith('image/')) {
      thumb.src = url.create(file);
      thumb.hidden = false;
    }
    pending.appendChild(tile);
  });
}

// Whether a drag carries files, rather than text or a row of the page.
function carriesFiles(e) {
  return Boolean(e.dataTransfer && Array.from(e.dataTransfer.types || []).includes('Files'));
}

const BUTTONS = '[data-file-up], [data-file-down]';

// Everything below only makes sense with a document. It lives in a function so
// this file can also be `require`d by its Node unit test, which has no DOM.
// Every listener is on the document: runtime navigation replaces the page
// without running this script again.
function install(
  doc = document,
  transfer = () => new DataTransfer(),
  url = { create: (file) => URL.createObjectURL(file), revoke: (src) => URL.revokeObjectURL(src) },
) {
  doc.addEventListener('click', (e) => {
    const target = e.target && e.target.closest ? e.target : null;
    const button = target && target.closest(BUTTONS);
    if (button) {
      act(button);
      return;
    }
    const unpick = target && target.closest('[data-file-unpick]');
    const zone = unpick && unpick.closest('[data-file-drop]');
    const input = zone && inputOf(zone);
    if (!input) return;
    const index = Number(unpick.getAttribute('data-file-unpick'));
    hold(
      input,
      Array.from(input.files).filter((_, at) => at !== index),
      transfer,
    );
    input.focus();
  });

  doc.addEventListener('change', (e) => {
    const input = e.target;
    if (!input || input.type !== 'file' || !input.closest) return;
    const zone = input.closest('[data-file-drop]');
    if (zone) render(zone, input, url);
  });

  // A zone takes files dropped anywhere on it, its input included, so a drop
  // on the input joins its files as a drop beside it does.
  const zoneOf = (e) => (e.target && e.target.closest ? e.target.closest('[data-file-drop]') : null);
  doc.addEventListener('dragover', (e) => {
    const zone = zoneOf(e);
    const input = zone && inputOf(zone);
    if (!input || input.disabled || !carriesFiles(e)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    zone.setAttribute('data-dragging', '');
  });
  doc.addEventListener('dragleave', (e) => {
    const zone = zoneOf(e);
    if (zone && !(e.relatedTarget && zone.contains(e.relatedTarget))) {
      zone.removeAttribute('data-dragging');
    }
  });
  doc.addEventListener('drop', (e) => {
    const zone = zoneOf(e);
    const input = zone && inputOf(zone);
    if (!input || input.disabled || !carriesFiles(e)) return;
    e.preventDefault();
    zone.removeAttribute('data-dragging');
    hold(input, joined(input, e.dataTransfer.files), transfer);
  });

  // Dragging a row by its handle. The pointer is captured, so the moves keep
  // coming while it leaves the handle.
  let drag = null;
  doc.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    const handle = e.target && e.target.closest && e.target.closest('[data-file-handle]');
    const row = handle && handle.closest('[data-file-item]');
    const list = row && row.closest('[data-file-list]');
    if (!list) return;
    e.preventDefault();
    if (handle.setPointerCapture) handle.setPointerCapture(e.pointerId);
    row.setAttribute('data-dragging', '');
    drag = { list, row, pointer: e.pointerId, moved: false };
  });
  doc.addEventListener('pointermove', (e) => {
    if (!drag || e.pointerId !== drag.pointer) return;
    if (place(drag.list, drag.row, e.clientY)) drag.moved = true;
  });
  const end = (e) => {
    if (!drag || e.pointerId !== drag.pointer) return;
    drag.row.removeAttribute('data-dragging');
    if (drag.moved) changed(drag.list);
    drag = null;
  };
  doc.addEventListener('pointerup', end);
  doc.addEventListener('pointercancel', end);
}

if (typeof document !== 'undefined') install();

// Exposed for the Node unit test (`file_lists.test.js`); the guard keeps the
// export out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { act, install, joined, place, render };
}
