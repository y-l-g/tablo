// Unit tests for `choice_create.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/choice_create.test.js
//
// What the cases protect:
// * the framed create page asks for pick mode, keeping its own query;
// * a saved record becomes the select's value, added once, and announced;
// * a searchable choice's box shows the record's title;
// * a page carrying no saved record leaves the choice and the dialog alone;
// * Escape in the frame closes the dialog unless a control in it took the key;
// * the frame fits the pick page, an open list reaching past it included, and
//   keeps its own height on any other page;
// * a framed form starts on its first control, or its first invalid one;
// * the blank page a closed dialog's frame loads does not mark the frame loaded;
// * closing drops the frame's unsaved-changes mark before emptying it, so
//   leaving the framed form does not ask;
// * a plain click opens the dialog on the framed page, and a modified click
//   leaves the link to the browser.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { install, pickUrl, choose, landed, pageHeight, firstControl } = require('./choice_create');

// A stand-in `Event`, which Node's runtime provides but the test pins.
global.Event = class {
  constructor(type, init = {}) {
    this.type = type;
    this.bubbles = Boolean(init.bubbles);
  }
};

// A select holding `values`, recording the events it announces.
function select(values, filter = null) {
  const announced = [];
  const owner = {
    createElement: () => ({ value: '', textContent: '' }),
  };
  const node = {
    options: values.map((value) => ({ value, textContent: value })),
    value: '',
    ownerDocument: owner,
    announced,
    focused: false,
    focus() {
      this.focused = true;
    },
    appendChild(option) {
      this.options.push(option);
    },
    dispatchEvent(event) {
      announced.push(event);
    },
    closest: (selector) =>
      selector === '[data-select-filterable]'
        ? { querySelector: (inner) => (inner === '[data-options-filter]' ? filter : null) }
        : null,
  };
  return node;
}

// A dialog naming the select `id`, recording whether it closed.
function dialog(id) {
  return {
    closed: false,
    getAttribute: (name) => (name === 'data-create-option-select' ? id : null),
    close() {
      this.closed = true;
    },
  };
}

// A framed page holding `picked`, the element carrying a saved record, if any.
function page(picked) {
  return { querySelector: (selector) => (selector === '[data-picked]' ? picked : null) };
}

// The element the page a save lands on carries.
function picked(value, label) {
  const attrs = { 'data-picked-value': value, 'data-picked-label': label };
  return { getAttribute: (name) => attrs[name] };
}

test('the framed create page asks for pick mode and keeps its query', () => {
  assert.equal(
    pickUrl('/admin/authors/-/create?team=7', 'http://localhost/admin/posts/-/create'),
    'http://localhost/admin/authors/-/create?team=7&pick=1',
  );
});

test('a saved record becomes the value, once, and is announced', () => {
  const field = select(['a']);
  choose(field, 'b', 'Bee');
  assert.deepEqual(
    field.options.map((option) => [option.value, option.textContent]),
    [
      ['a', 'a'],
      ['b', 'Bee'],
    ],
  );
  assert.equal(field.value, 'b');
  assert.equal(field.announced.length, 1);
  assert.equal(field.announced[0].type, 'change');
  assert.equal(field.announced[0].bubbles, true);

  choose(field, 'b', 'Bee');
  assert.equal(field.options.length, 2, 'an offered record is not added again');
});

test("a searchable choice's box shows the record's title", () => {
  const filter = { value: 'be' };
  choose(select([], filter), 'b', 'Bee');
  assert.equal(filter.value, 'Bee');
});

test('the landing page makes the record the choice and closes the dialog', () => {
  const field = select([]);
  const doc = { getElementById: (id) => (id === 'author_id' ? field : null) };
  const shown = dialog('author_id');
  assert.equal(landed(doc, shown, page(picked('42', 'Ada'))), true);
  assert.equal(field.value, '42');
  assert.equal(shown.closed, true);
  assert.equal(field.focused, true, 'focus returns to the choice');
});

test('a page carrying no saved record leaves the choice and the dialog', () => {
  const field = select([]);
  const doc = { getElementById: () => field };
  const shown = dialog('author_id');
  assert.equal(landed(doc, shown, page(null)), false);
  assert.equal(field.value, '');
  assert.equal(shown.closed, false);
});

// A node spanning `top` to `bottom`.
function box(top, bottom, init = {}) {
  return { getBoundingClientRect: () => ({ top, bottom }), ...init };
}

// A framed page whose pick block is `block`, holding the searchable lists `lists`.
function sized(block, lists = []) {
  return {
    querySelector: (selector) => (selector === '[data-pick-page]' ? block : null),
    querySelectorAll: (selector) => (selector === '[data-options-list]' ? lists : []),
  };
}

test('the frame fits the pick block, rounding up', () => {
  assert.equal(pageHeight(sized(box(0, 319.2))), 320);
});

test('an open list past the block makes the frame taller, a closed one does not', () => {
  assert.equal(pageHeight(sized(box(0, 300), [box(250, 490)])), 490);
  assert.equal(pageHeight(sized(box(0, 300), [box(250, 490, { hidden: true })])), 300);
});

test('a page with no pick block keeps the height the frame has', () => {
  assert.equal(pageHeight(sized(null)), null);
});

test('a framed form starts on its first invalid control, else its first', () => {
  const first = {};
  const invalid = {};
  const finding = (found) => ({
    querySelector: (selector) => (selector.includes('aria-invalid') ? found : first),
  });
  assert.equal(firstControl(finding(invalid)), invalid);
  assert.equal(firstControl(finding(null)), first);
});

// A link naming the dialog `create`, whose frame records its source.
function opening() {
  const frame = { src: '' };
  const shown = {
    opened: false,
    querySelector: (selector) => (selector === '[data-create-option-frame]' ? frame : null),
    showModal() {
      this.opened = true;
    },
  };
  const attrs = { 'data-create-option': 'create', href: '/admin/authors/-/create' };
  const link = { getAttribute: (name) => attrs[name] };
  const doc = listenerDocument({
    baseURI: 'http://localhost/admin/posts/-/create',
    getElementById: (id) => (id === 'create' ? shown : null),
  });
  install(doc);
  const click = (init) => {
    const event = {
      button: 0,
      target: { closest: (selector) => (selector === 'a[data-create-option]' ? link : null) },
      prevented: false,
      preventDefault() {
        this.prevented = true;
      },
      ...init,
    };
    for (const handler of doc.listeners('click')) handler(event);
    return event;
  };
  return { frame, shown, click };
}

test('a plain click opens the dialog on the framed create page', () => {
  const { frame, shown, click } = opening();
  assert.equal(click({}).prevented, true);
  assert.equal(shown.opened, true);
  assert.equal(frame.src, 'http://localhost/admin/authors/-/create?pick=1');
});

test('a modified click leaves the link to the browser', () => {
  const { frame, shown, click } = opening();
  assert.equal(click({ ctrlKey: true }).prevented, false);
  assert.equal(shown.opened, false);
  assert.equal(frame.src, '');
});

// The frame of a loaded dialog: a page whose form `dirty` marks as changed,
// recording the listeners the script binds on it.
function framed() {
  const form = {
    dirty: true,
    removeAttribute(name) {
      if (name === 'data-form-dirty') this.dirty = false;
    },
  };
  const page = listenerDocument({
    querySelector: () => null,
    querySelectorAll: (selector) => (selector === 'form[data-form-dirty]' && form.dirty ? [form] : []),
  });
  const shown = {
    open: true,
    closed: false,
    matches: (selector) => selector === '[data-create-option-dialog]',
    querySelector: (selector) => (selector === '[data-create-option-frame]' ? frame : null),
    close() {
      this.closed = true;
    },
  };
  const frame = {
    src: 'http://localhost/admin/authors/-/create?pick=1',
    contentDocument: page,
    style: {},
    loaded: false,
    setAttribute() {
      this.loaded = true;
    },
    removeAttribute() {
      this.loaded = false;
    },
    matches: (selector) => selector === '[data-create-option-frame]',
    closest: (selector) => (selector === '[data-create-option-dialog]' ? shown : null),
  };
  const doc = listenerDocument({ getElementById: () => null });
  install(doc);
  const load = () => {
    for (const handler of doc.listeners('load')) handler({ target: frame });
  };
  load();
  const key = (init) => {
    for (const handler of page.listeners('keydown')) handler({ key: 'Escape', defaultPrevented: false, ...init });
  };
  const close = () => {
    for (const handler of doc.listeners('close')) handler({ target: shown });
  };
  return { form, frame, shown, load, key, close };
}

test('Escape in the frame closes the dialog unless a control took it', () => {
  const taken = framed();
  taken.key({ defaultPrevented: true });
  assert.equal(taken.shown.closed, false, "an open combobox's Escape keeps the dialog");
  const free = framed();
  free.key({});
  assert.equal(free.shown.closed, true);
});

test('closing the dialog drops the unsaved-changes mark and empties the frame', () => {
  const { form, frame, close } = framed();
  frame.style.height = '320px';
  close();
  assert.equal(form.dirty, false);
  assert.equal(frame.src, 'about:blank');
  assert.equal(frame.style.height, '', 'the next opening starts from the frame\'s own height');
});

test('the blank page a closed dialog loads leaves the frame unmarked', () => {
  const { frame, shown, load } = framed();
  assert.equal(frame.loaded, true, 'the page an open dialog loads marks the frame');
  shown.open = false;
  frame.loaded = false;
  load();
  assert.equal(frame.loaded, false);
});
