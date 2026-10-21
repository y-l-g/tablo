// Unit tests for `rich_text.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/rich_text.test.js
//
// What the cases protect:
// * the editor registers only the marks and nodes the toolbar's tools produce;
// * mounting hides the textarea and stops it requiring a value, shows the toolbar, and copies
//   each change back into the textarea, an empty document as nothing;
// * a press runs its tool on the editor, and the arrow keys move the toolbar's one tab stop,
//   which never rests on a disabled button;
// * a link goes only to a URL the server keeps;
// * a control a morph kept but emptied mounts again from the new page's value, not the edits the
//   old page left in the textarea, and only a mutation touching a control rescans.

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  allowedUri,
  mount,
  press,
  rove,
  scan,
  starterKit,
  touchesControl,
} = require('./rich_text');

// An element stand-in recording its attributes, found by `closest` through `parents`.
function element(attrs = {}, parents = {}) {
  return {
    attrs: { ...attrs },
    hidden: false,
    disabled: false,
    tabIndex: -1,
    getAttribute(name) {
      return name in this.attrs ? this.attrs[name] : null;
    },
    setAttribute(name, value) {
      this.attrs[name] = value;
    },
    closest(selector) {
      return parents[selector] || null;
    },
    focus() {
      this.focused = true;
    },
  };
}

// A fake TipTap module whose editor records how it was built and the commands it ran.
function tiptap() {
  const ran = [];
  class Editor {
    constructor(options) {
      this.options = options;
      this.view = { dom: { editorOf: options.element } };
      options.element.children = [this.view.dom];
      this.isEditable = options.editable;
      this.isEmpty = false;
      this.html = '';
    }
    getHTML() {
      return this.html;
    }
    destroy() {
      this.destroyed = true;
    }
    isActive(name) {
      return name === 'bold';
    }
    can() {
      return { undo: () => false, redo: () => true };
    }
    chain() {
      const chain = new Proxy(
        {},
        {
          get: (_, command) => () => {
            ran.push(command);
            return chain;
          },
        },
      );
      return chain;
    }
  }
  return {
    ran,
    Editor,
    StarterKit: { configure: (options) => ({ starterKit: options }) },
    Placeholder: { configure: (options) => ({ placeholder: options }) },
  };
}

// A control holding `value`, with a toolbar of `tools`.
function control(value, tools) {
  const buttons = tools.map((tool) => {
    const attrs = { 'data-rich-text-tool': tool };
    if (tool !== 'undo' && tool !== 'redo') attrs['aria-pressed'] = 'false';
    return element(attrs);
  });
  const toolbar = element();
  toolbar.hidden = true;
  toolbar.querySelector = () => buttons[0];
  toolbar.querySelectorAll = () => buttons;
  const content = element();
  content.hidden = true;
  content.children = [];
  content.contains = (node) => content.children.includes(node);
  const source = element({ placeholder: 'Write…', 'aria-required': 'true' });
  source.id = 'body';
  source.value = value;
  source.required = true;
  const root = element({ 'data-rich-text-tools': tools.join(' ') });
  root.isConnected = true;
  root.querySelector = (selector) =>
    ({
      '[data-rich-text-source]': source,
      '[data-rich-text-content]': content,
      '[data-rich-text-toolbar]': toolbar,
    })[selector];
  for (const button of buttons) {
    button.closest = (selector) =>
      ({ '[data-rich-text]': root, '[data-rich-text-toolbar]': toolbar })[selector] || null;
  }
  return { root, source, content, toolbar, buttons };
}

test('the editor registers only what the tools produce', () => {
  const options = starterKit(new Set(['bold', 'heading-2', 'ordered-list']));
  assert.deepEqual(options.bold, {});
  assert.equal(options.italic, false);
  assert.equal(options.link, false);
  assert.deepEqual(options.heading, { levels: [2] });
  assert.deepEqual(options.orderedList, {});
  assert.equal(options.bulletList, false);
  assert.deepEqual(options.listItem, {}, 'a list needs its items');
  assert.equal(options.codeBlock, false);
  assert.equal(starterKit(new Set()).heading, false);
  assert.equal(starterKit(new Set()).listItem, false);
});

test('mounting swaps the textarea for the editor, and changes post through it', () => {
  const module = tiptap();
  const { root, source, content, toolbar, buttons } = control('<p>Hi</p>', ['bold', 'undo']);
  const editor = mount(root, module);
  assert.equal(editor.options.content, '<p>Hi</p>');
  assert.equal(editor.options.element, content);
  assert.equal(source.hidden, true);
  assert.equal(source.required, false, 'a hidden required textarea would block the submit');
  assert.equal(content.hidden, false);
  assert.equal(toolbar.hidden, false);
  assert.equal(buttons[0].tabIndex, 0, 'the toolbar holds one tab stop');
  assert.equal(buttons[0].attrs['aria-pressed'], 'true');
  assert.equal(buttons[1].disabled, true, 'nothing to undo yet');
  assert.equal(editor.options.editorProps.attributes['aria-required'], 'true');
  assert.deepEqual(editor.options.extensions[1], { placeholder: { placeholder: 'Write…' } });
  assert.equal(mount(root, module), editor, 'a control mounts once');

  editor.html = '<p>Hello</p>';
  editor.options.onUpdate({ editor });
  assert.equal(source.value, '<p>Hello</p>');
  editor.isEmpty = true;
  editor.options.onUpdate({ editor });
  assert.equal(source.value, '', 'an empty document posts nothing');
});

test('a disabled textarea mounts a read-only editor', () => {
  const { root, source } = control('<p>Hi</p>', ['bold']);
  source.disabled = true;
  assert.equal(mount(root, tiptap()).options.editable, false);
});

test('a press runs its tool on the editor', () => {
  const module = tiptap();
  const { root, buttons } = control('', ['bold', 'italic']);
  mount(root, module);
  press(buttons[1], {});
  assert.deepEqual(module.ran, ['focus', 'toggleItalic', 'run']);
});

test('a link press asks for the URL, and an empty answer removes the link', () => {
  const module = tiptap();
  const { root, buttons } = control('', ['link']);
  const editor = mount(root, module);
  editor.getAttributes = () => ({ href: 'https://old.test' });
  press(buttons[0], { prompt: () => ' https://new.test ' });
  assert.deepEqual(module.ran, ['focus', 'extendMarkRange', 'setLink', 'run']);
  module.ran.length = 0;
  press(buttons[0], { prompt: () => '' });
  assert.deepEqual(module.ran, ['focus', 'extendMarkRange', 'unsetLink', 'run']);
  module.ran.length = 0;
  press(buttons[0], { prompt: () => null });
  assert.deepEqual(module.ran, ['focus', 'run'], 'a cancel changes nothing');
});

test('the arrow keys move the one tab stop along the toolbar', () => {
  const { root, buttons } = control('', ['bold', 'italic', 'undo']);
  mount(root, tiptap());
  buttons[2].disabled = true;
  assert.equal(rove(buttons[0], 'ArrowRight'), true);
  assert.equal(buttons[0].tabIndex, -1);
  assert.equal(buttons[1].tabIndex, 0);
  assert.equal(buttons[1].focused, true);
  rove(buttons[1], 'ArrowRight');
  assert.equal(buttons[0].tabIndex, 0, 'it wraps past a disabled tool');
  assert.equal(rove(buttons[0], 'a'), false, 'other keys stay with the button');
});

test('the tab stop leaves a button as it disables', () => {
  const { root, buttons } = control('', ['undo', 'redo', 'bold']);
  const editor = mount(root, tiptap());
  assert.equal(buttons[0].tabIndex, -1, 'nothing to undo, so the stop skips Undo');
  assert.equal(buttons[1].tabIndex, 0);
  editor.can = () => ({ undo: () => true, redo: () => false });
  editor.options.onTransaction({ editor });
  assert.deepEqual(
    buttons.map((button) => button.tabIndex),
    [0, -1, -1],
    'Redo disabled, the stop moves to the first enabled button',
  );
});

test('a link goes only to a URL the server keeps', () => {
  for (const url of ['https://x.test', 'HTTP://x.test', 'mailto:a@x.test', '/a', 'page#b']) {
    assert.equal(allowedUri(url), true, url);
  }
  for (const url of ['tel:123', 'ftp://x.test', 'javascript:alert(1)', ' xmpp:a@x.test']) {
    assert.equal(allowedUri(url), false, url);
  }
  assert.equal(starterKit(new Set(['link'])).link.isAllowedUri, allowedUri);
});

// A page holding `controls`, whose head names the TipTap module.
function page(controls) {
  return {
    querySelector: (selector) =>
      selector === 'meta[name="tablo-tiptap"]' ? element({ content: '/tiptap.js' }) : null,
    querySelectorAll: () => controls.map((c) => c.root),
  };
}

test('a control a morph emptied mounts again from the new page', async () => {
  const module = tiptap();
  const first = control('<p>Post 3</p>', ['bold']);
  first.source.defaultValue = '<p>Post 3</p>';
  const doc = page([first]);
  scan(doc, async () => module);
  await new Promise(setImmediate);
  const editor = first.root.richTextEditor;
  assert.ok(editor, 'the scan mounts the control');
  first.source.value = '<p>Post 3, edited</p>';

  scan(doc, async () => module);
  await new Promise(setImmediate);
  assert.equal(first.root.richTextEditor, editor, 'an attached editor stays');

  // The morph keeps the elements, empties the content and shows the textarea again.
  first.content.children = [];
  first.content.hidden = true;
  first.source.hidden = false;
  first.source.defaultValue = '<p>Post 4</p>';
  scan(doc, async () => module);
  await new Promise(setImmediate);
  assert.equal(editor.destroyed, true);
  const again = first.root.richTextEditor;
  assert.notEqual(again, editor);
  assert.equal(again.options.content, '<p>Post 4</p>');
  assert.equal(first.source.value, '<p>Post 4</p>', 'the old edits never post');
  assert.equal(first.source.hidden, true);
});

test('only a mutation touching a control rescans', () => {
  const node = (matches, holds = false) => ({
    nodeType: 1,
    matches: (selector) => matches === selector,
    querySelector: () => (holds ? {} : null),
  });
  const record = (target, added = [], removed = []) => ({
    target,
    addedNodes: added,
    removedNodes: removed,
  });
  const typing = record(node('.ProseMirror'), [{ nodeType: 3 }]);
  assert.equal(touchesControl([typing]), false, 'a keystroke in an editor');
  assert.equal(touchesControl([record(node('main'), [node('div', true)])]), true);
  assert.equal(touchesControl([record(node('[data-rich-text-content]'))]), true);
});
