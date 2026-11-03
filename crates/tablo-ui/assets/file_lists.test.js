// Unit tests for `file_lists.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/file_lists.test.js
//
// What the cases protect:
// * an item's buttons move it one place, and the items' order is all the
//   server reads the list's order from;
// * a dragged item lands where the pointer lets go, in a column of rows and in
//   a grid of tiles, and marks the form changed once;
// * a mouse drags an item from anywhere but its controls, a finger only by its
//   grip;
// * a move fires a bubbling `change`, so the leave-page guard counts it;
// * a button at either end, or outside a file list, moves nothing;
// * files dropped on a zone join a list's input after its own, and replace a
//   single field's; a drag carrying no files is left to the browser;
// * a picked file's remove button takes that file, and no other, out of what
//   the input posts;
// * a picked file shows a thumbnail only in an image zone, and only for an
//   image, and its size;
// * the single wiring pass: one listener per event type, so an event acts once.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');

// --- an element stand-in -----------------------------------------------------

// Only as wide as the script needs: attributes, children, the sibling,
// ancestor and descendant walks, a box for the drag, and selectors made of
// `tag`, `[attr]` or `tag[attr="value"]`, joined by commas.
class Element {
  constructor(attributes = {}, children = [], tag = 'div') {
    this.tagName = tag.toUpperCase();
    this.attrs = new Map(Object.entries(attributes));
    this.children = [];
    this.parent = null;
    this.focused = false;
    this.events = [];
    this.textContent = '';
    this.hidden = false;
    this.top = 0;
    this.left = 0;
    this.width = 100;
    children.forEach((child) => this.appendChild(child));
  }

  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }

  hasAttribute(name) {
    return this.attrs.has(name);
  }

  setAttribute(name, value) {
    this.attrs.set(name, value);
  }

  removeAttribute(name) {
    this.attrs.delete(name);
  }

  appendChild(child) {
    if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1);
    child.parent = this;
    this.children.push(child);
  }

  insertBefore(child, before) {
    if (!before) return this.appendChild(child);
    this.children.splice(this.children.indexOf(child), 1);
    this.children.splice(this.children.indexOf(before), 0, child);
  }

  replaceChildren() {
    this.children = [];
  }

  get previousElementSibling() {
    const siblings = this.parent.children;
    return siblings[siblings.indexOf(this) - 1] || null;
  }

  get nextElementSibling() {
    const siblings = this.parent.children;
    return siblings[siblings.indexOf(this) + 1] || null;
  }

  getBoundingClientRect() {
    return { top: this.top, bottom: this.top + 10, height: 10, left: this.left, width: this.width };
  }

  matches(selector) {
    return selector.split(',').some((one) => {
      const [, tag, name, value] = /^(\w*)(?:\[([\w-]+)(?:="([^"]*)")?\])?$/.exec(one.trim());
      if (tag && this.tagName !== tag.toUpperCase()) return false;
      if (name && !this.hasAttribute(name)) return false;
      return value === undefined || this.getAttribute(name) === value;
    });
  }

  closest(selector) {
    for (let node = this; node; node = node.parent) {
      if (node.matches(selector)) return node;
    }
    return null;
  }

  contains(node) {
    for (; node; node = node.parent) if (node === this) return true;
    return false;
  }

  querySelectorAll(selector) {
    return this.children.flatMap((child) => [
      ...(child.matches(selector) ? [child] : []),
      ...child.querySelectorAll(selector),
    ]);
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  cloneNode() {
    const copy = new Element(
      Object.fromEntries(this.attrs),
      this.children.map((child) => child.cloneNode()),
      this.tagName,
    );
    copy.hidden = this.hidden;
    return copy;
  }

  focus() {
    this.focused = true;
  }

  dispatchEvent(event) {
    this.events.push(event);
  }
}

// A file list holding an item per path, each a handle with its up and down
// buttons, its grip and its name, the items ten pixels tall and stacked.
function fileList(paths) {
  const rows = paths.map((path, index) => {
    const row = new Element({ 'data-file-item': path, 'data-file-handle': '' }, [
      new Element({ 'data-file-up': '' }, [], 'button'),
      new Element({ 'data-file-down': '' }, [], 'button'),
      new Element({ 'data-file-grip': '' }, [], 'span'),
      new Element({}, [new Element({ href: path }, [], 'a')]),
    ]);
    row.top = index * 10;
    return row;
  });
  return new Element({ 'data-file-list': 'photos' }, rows);
}

// The same list as a grid of tiles forty pixels wide, `columns` to a line.
function fileGrid(paths, columns) {
  const list = fileList(paths);
  list.children.forEach((tile, index) => {
    tile.width = 40;
    tile.left = (index % columns) * 40;
    tile.top = Math.floor(index / columns) * 10;
  });
  return list;
}

function order(list) {
  return list.children.map((row) => row.getAttribute('data-file-item'));
}

function button(list, path, kind) {
  const row = list.children.find((row) => row.getAttribute('data-file-item') === path);
  if (kind === 'item') return row;
  if (kind === 'link') return row.children[3].children[0];
  return row.children[{ up: 0, down: 1, grip: 2 }[kind]];
}

// A drop zone around a file input holding `files`, with the picked-file
// template the server renders.
function dropZone(files, { multiple = true, image = false } = {}) {
  const input = new Element({ type: 'file' }, [], 'input');
  input.id = 'photos';
  input.type = 'file';
  input.multiple = multiple;
  input.files = files;
  const tile = new Element({}, [
    new Element({ 'data-file-picked-thumb': '' }, [], 'img'),
    new Element({ 'data-file-picked-name': '' }, [], 'span'),
    new Element({ 'data-file-picked-size': '' }, [], 'span'),
    new Element({ 'data-file-unpick': '' }, [], 'button'),
  ]);
  tile.children[0].hidden = true;
  const template = new Element({ 'data-file-picked-template': '' }, [], 'template');
  template.content = { firstElementChild: tile };
  const zone = new Element({ 'data-file-drop': '', ...(image ? { 'data-file-image': '' } : {}) }, [
    input,
    new Element({ 'data-file-pending': '' }),
    template,
  ]);
  return { zone, input };
}

const file = (name, type = 'image/png', size = 1500) => ({ name, type, size });
const names = (input) => input.files.map((f) => f.name);

// What the browser's `DataTransfer` gives: a list the added files land in.
function transfer() {
  const files = [];
  return { items: { add: (f) => files.push(f) }, files };
}

const urls = { create: (f) => `blob:${f.name}`, revoke: () => {} };

// The script installed on a stand-in document.
function installed() {
  const doc = listenerDocument();
  const { install } = require('./file_lists.js');
  install(doc, transfer, urls);
  const fire = (type, event) => doc.listeners(type).forEach((handler) => handler(event));
  return { doc, fire };
}

// A drag event carrying `files` over `target`.
function dragEvent(target, files, types = ['Files']) {
  return {
    target,
    dataTransfer: { types, files, dropEffect: 'none' },
    prevented: false,
    preventDefault() {
      this.prevented = true;
    },
  };
}

// --- the cases -----------------------------------------------------------------

const { act, joined, place, render, size } = require('./file_lists.js');

test('a row moves one place up or down', () => {
  const list = fileList(['/a', '/b', '/c']);
  assert.equal(act(button(list, '/c', 'up')), true);
  assert.deepEqual(order(list), ['/a', '/c', '/b']);
  assert.equal(act(button(list, '/a', 'down')), true);
  assert.deepEqual(order(list), ['/c', '/a', '/b']);
  assert.equal(button(list, '/a', 'down').focused, true, 'the moved button keeps focus');
  assert.deepEqual(
    list.events.map((event) => [event.type, event.bubbles]),
    [
      ['change', true],
      ['change', true],
    ],
    'each move marks the form changed',
  );
});

test('a row at either end stays put', () => {
  const list = fileList(['/a', '/b']);
  act(button(list, '/a', 'up'));
  act(button(list, '/b', 'down'));
  assert.deepEqual(order(list), ['/a', '/b']);
  assert.deepEqual(list.events, [], 'nothing moved, nothing changed');
});

test('a button outside a file list does nothing', () => {
  const stray = new Element({ 'data-file-up': '' });
  new Element({}, [stray]);
  assert.equal(act(stray), false);
});

test('a dragged row lands before the first row whose middle is below the pointer', () => {
  const list = fileList(['/a', '/b', '/c']);
  const row = list.children[0];
  assert.equal(place(list, row, 90, 16), true, 'past the middle of /b');
  assert.deepEqual(order(list), ['/b', '/a', '/c']);
  assert.equal(place(list, row, 90, 16), false, 'the same spot again moves nothing');
  assert.equal(place(list, row, 0, 99), true);
  assert.deepEqual(order(list), ['/b', '/c', '/a']);
  assert.equal(place(list, row, 0, 99), false, 'already last');
});

test('a dragged tile lands before the first tile the pointer comes before, line by line', () => {
  // /a /b
  // /c /d
  const list = fileGrid(['/a', '/b', '/c', '/d'], 2);
  const tile = list.children[0];
  list.width = 80;
  assert.equal(place(list, tile, 70, 5), true, 'past the middle of /b, on its line');
  assert.deepEqual(order(list), ['/b', '/a', '/c', '/d']);
  assert.equal(place(list, tile, 10, 15), false, 'left of /c is where it already is');
  assert.equal(place(list, tile, 50, 15), true, 'left of the middle of /d');
  assert.deepEqual(order(list), ['/b', '/c', '/a', '/d']);
});

test('a mouse drags an item from anywhere but its controls, a finger by its grip', () => {
  const { fire } = installed();
  const list = fileList(['/a', '/b']);
  const press = (target, pointerType) =>
    fire('pointerdown', { target, pointerType, button: 0, pointerId: 1, preventDefault() {} });
  const dragging = () => list.children[0].hasAttribute('data-dragging');
  const release = () => fire('pointerup', { pointerId: 1 });

  press(button(list, '/a', 'link'), 'mouse');
  assert.equal(dragging(), false, 'a link stays a link');
  press(button(list, '/a', 'up'), 'mouse');
  assert.equal(dragging(), false, 'a button stays a button');
  press(button(list, '/a', 'item'), 'touch');
  assert.equal(dragging(), false, 'a finger on the item scrolls the page');
  press(button(list, '/a', 'grip'), 'touch');
  assert.equal(dragging(), true);
  release();
  press(button(list, '/a', 'item'), 'mouse');
  assert.equal(dragging(), true);
  release();
});

test('dragging a handle reorders the list and marks the form changed once', () => {
  const { fire } = installed();
  const list = fileList(['/a', '/b', '/c']);
  const handle = button(list, '/a', 'grip');
  const down = { target: handle, button: 0, pointerId: 1, preventDefault() {} };
  fire('pointerdown', down);
  assert.equal(list.children[0].hasAttribute('data-dragging'), true);
  fire('pointermove', { pointerId: 1, clientX: 90, clientY: 16 });
  fire('pointermove', { pointerId: 2, clientX: 90, clientY: 99 });
  fire('pointermove', { pointerId: 1, clientX: 90, clientY: 26 });
  fire('pointerup', { pointerId: 1 });
  assert.deepEqual(order(list), ['/b', '/c', '/a'], 'another pointer moves nothing');
  assert.equal(list.children[2].hasAttribute('data-dragging'), false);
  assert.deepEqual(
    list.events.map((event) => event.type),
    ['change'],
  );
});

test('a drop joins a list input after its files, and replaces a single input', () => {
  const list = { multiple: true, files: [file('a.png')] };
  assert.deepEqual(
    joined(list, [file('b.png'), file('c.png')]).map((f) => f.name),
    ['a.png', 'b.png', 'c.png'],
  );
  const single = { multiple: false, files: [file('a.png')] };
  assert.deepEqual(
    joined(single, [file('b.png'), file('c.png')]).map((f) => f.name),
    ['b.png'],
  );
});

test('a drop of files fills the input; a drag of anything else is left alone', () => {
  const { fire } = installed();
  const { zone, input } = dropZone([file('a.png')]);
  const over = dragEvent(input, []);
  fire('dragover', over);
  assert.equal(over.prevented, true, 'the zone, its input included, takes the drop');
  assert.equal(zone.hasAttribute('data-dragging'), true);

  const drop = dragEvent(zone, [file('b.png')]);
  fire('drop', drop);
  assert.equal(drop.prevented, true);
  assert.deepEqual(names(input), ['a.png', 'b.png']);
  assert.equal(zone.hasAttribute('data-dragging'), false);
  assert.deepEqual(
    input.events.map((event) => [event.type, event.bubbles]),
    [['change', true]],
    'the form sees the input change',
  );

  const text = dragEvent(zone, [], ['text/plain']);
  fire('drop', text);
  assert.equal(text.prevented, false);
  assert.deepEqual(names(input), ['a.png', 'b.png']);

  input.disabled = true;
  const refused = dragEvent(zone, [file('c.png')]);
  fire('drop', refused);
  assert.deepEqual(names(input), ['a.png', 'b.png'], 'a disabled input takes nothing');
});

test("a picked file's remove button takes that file out of the input", () => {
  const { fire } = installed();
  const { zone, input } = dropZone([file('a.png'), file('b.png'), file('c.png')]);
  render(zone, input, urls);
  const unpick = zone.querySelectorAll('[data-file-unpick]')[1];
  assert.equal(unpick.getAttribute('aria-describedby'), 'photos-picked-1');
  fire('click', { target: unpick });
  assert.deepEqual(names(input), ['a.png', 'c.png']);
  assert.equal(input.focused, true);

  fire('change', { target: input });
  assert.deepEqual(
    zone.querySelectorAll('[data-file-picked-name]').map((name) => name.textContent),
    ['a.png', 'c.png'],
    'the list follows the input',
  );
});

test('a picked file shows a thumbnail in an image zone, and only for an image', () => {
  const picked = [file('a.png'), file('notes.pdf', 'application/pdf')];
  const thumbs = (zone) =>
    zone.querySelectorAll('[data-file-picked-thumb]').map((img) => (img.hidden ? null : img.src));
  const images = dropZone(picked, { image: true });
  render(images.zone, images.input, urls);
  assert.deepEqual(thumbs(images.zone), ['blob:a.png', null]);
  const files = dropZone(picked);
  render(files.zone, files.input, urls);
  assert.deepEqual(thumbs(files.zone), [null, null]);
  assert.deepEqual(
    files.zone.querySelectorAll('[data-file-picked-size]').map((bytes) => bytes.textContent),
    [size(1500), size(1500)],
  );
});

test('a size reads in kilobytes, or megabytes past a thousand of them', () => {
  assert.equal(size(1500, 'en'), '2 kB');
  assert.equal(size(10, 'en'), '1 kB', 'never zero');
  assert.equal(size(2_500_000, 'en'), '2.5 MB');
});

test('one listener per event type, so an event acts once', () => {
  const document = listenerDocument();
  global.document = document;
  delete require.cache[require.resolve('./file_lists.js')];
  require('./file_lists.js');
  delete global.document;
  assert.deepEqual(document.types(), [
    'click',
    'change',
    'dragover',
    'dragleave',
    'drop',
    'pointerdown',
    'pointermove',
    'pointerup',
    'pointercancel',
  ]);
  for (const type of document.types()) assert.equal(document.listeners(type).length, 1);

  const list = fileList(['/a', '/b', '/c']);
  document.listeners('click')[0]({ target: button(list, '/c', 'up') });
  assert.deepEqual(order(list), ['/a', '/c', '/b']);
});
