// Rows for Tablo repeaters.
//
// A repeater renders `[data-repeater]` holding:
//
// * a hidden `input[data-repeater-order]`, posting the numbers of its rows in
//   the order they show;
// * an `ol[data-repeater-rows]` of `li[data-repeater-row="<n>"]`, whose
//   controls post under `<key>.<n>.`;
// * a `template[data-repeater-blank]` holding one blank row, whose keys, ids
//   and `data-repeater-row` carry `__row__` for the row's number;
// * `data-repeater-next`, the next number no row has used.
//
// The server reads the rows in the order the hidden input lists them, so
// adding, removing and moving a row touch only the list and that input: a row
// keeps its number, and with it its keys and ids, wherever it moves. Each
// button is `type="button"`, so none submits the form.

const ROW = '__row__';

// Give every element under `root`, `root` included, the row number `row` in
// place of `ROW`.
function numberRow(root, row) {
  for (const attr of Array.from(root.attributes || [])) {
    if (attr.value.includes(ROW)) {
      root.setAttribute(attr.name, attr.value.split(ROW).join(String(row)));
    }
  }
  for (const child of Array.from(root.children || [])) numberRow(child, row);
}

// The hidden input's value: the rows' numbers, in the order they show.
function rowOrder(rows) {
  return rows.map((row) => row.getAttribute('data-repeater-row')).join(',');
}

// The parts of the repeater holding `node`.
function partsOf(node) {
  const repeater = node.closest('[data-repeater]');
  if (!repeater) return null;
  return {
    repeater,
    order: repeater.querySelector('[data-repeater-order]'),
    list: repeater.querySelector('[data-repeater-rows]'),
    blank: repeater.querySelector('template[data-repeater-blank]'),
    add: repeater.querySelector('[data-repeater-add]'),
  };
}

// Append a copy of the blank row, numbered with the next unused number, and
// focus its first control.
function addRow(parts) {
  const row = parts.repeater.getAttribute('data-repeater-next');
  const copy = parts.blank.content.cloneNode(true);
  const added = copy.firstElementChild;
  numberRow(added, row);
  parts.repeater.setAttribute('data-repeater-next', String(Number(row) + 1));
  parts.list.appendChild(copy);
  const first = added.querySelector('input:not([type="hidden"]), select, textarea');
  if (first) first.focus();
}

// Run the repeater button `button`; `false` when it is none.
function act(button) {
  const parts = partsOf(button);
  if (!parts) return false;
  if (button.hasAttribute('data-repeater-add')) {
    addRow(parts);
  } else {
    const row = button.closest('[data-repeater-row]');
    if (!row) return false;
    if (button.hasAttribute('data-repeater-remove')) {
      row.remove();
      parts.add.focus();
    } else if (button.hasAttribute('data-repeater-up')) {
      const previous = row.previousElementSibling;
      if (previous) parts.list.insertBefore(row, previous);
      button.focus();
    } else if (button.hasAttribute('data-repeater-down')) {
      const next = row.nextElementSibling;
      if (next) parts.list.insertBefore(next, row);
      button.focus();
    }
  }
  parts.order.value = rowOrder(Array.from(parts.list.children));
  return true;
}

const BUTTONS =
  '[data-repeater-add], [data-repeater-remove], [data-repeater-up], [data-repeater-down]';

// Everything below only makes sense with a document. It lives in a function so
// this file can also be `require`d by its Node unit test, which has no DOM.
function install() {
  document.addEventListener('click', (e) => {
    const button = e.target.closest && e.target.closest(BUTTONS);
    if (button) act(button);
  });
}

if (typeof document !== 'undefined') install();

// Exposed for the Node unit test (`repeaters.test.js`); the guard keeps the
// export out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { ROW, act, numberRow, rowOrder };
}
