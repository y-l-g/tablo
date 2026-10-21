// Rich text editors for Tablo's rich text controls (ADR-0031).
//
// A rich text control renders `[data-rich-text]` holding:
//
// * a hidden `[data-rich-text-toolbar]` of `button[data-rich-text-tool="<tool>"]`;
// * a hidden `[data-rich-text-content]`, where the editor mounts;
// * a `textarea[data-rich-text-source]` holding the HTML, which the form posts.
//
// Without this script the textarea edits the HTML itself, and the server cleans it either way.
// With it, the textarea hides and TipTap edits a document over the content element, copying the
// HTML back into the textarea on every change, so the form posts as a plain form. TipTap loads
// from the URL the head's `meta[name="tablo-tiptap"]` names the first time a page holds a
// control, so a page without one loads none of it. The editor registers only the extensions the toolbar's tools
// (`data-rich-text-tools`) need, so it cannot type or paste markup the server would drop.
//
// Hooks: `tablo-tiptap`, `[data-rich-text]`, `[data-rich-text-tools]`,
// `[data-rich-text-toolbar]`, `[data-rich-text-tool]`, `[data-rich-text-content]`,
// `[data-rich-text-source]`.

// What each tool runs on a command chain, and when its button shows pressed.
const TOOLS = {
  bold: { run: (c) => c.toggleBold(), active: (e) => e.isActive('bold') },
  italic: { run: (c) => c.toggleItalic(), active: (e) => e.isActive('italic') },
  underline: { run: (c) => c.toggleUnderline(), active: (e) => e.isActive('underline') },
  strike: { run: (c) => c.toggleStrike(), active: (e) => e.isActive('strike') },
  code: { run: (c) => c.toggleCode(), active: (e) => e.isActive('code') },
  link: { run: link, active: (e) => e.isActive('link') },
  'heading-1': heading(1),
  'heading-2': heading(2),
  'heading-3': heading(3),
  'bullet-list': { run: (c) => c.toggleBulletList(), active: (e) => e.isActive('bulletList') },
  'ordered-list': { run: (c) => c.toggleOrderedList(), active: (e) => e.isActive('orderedList') },
  blockquote: { run: (c) => c.toggleBlockquote(), active: (e) => e.isActive('blockquote') },
  'code-block': { run: (c) => c.toggleCodeBlock(), active: (e) => e.isActive('codeBlock') },
  'horizontal-rule': { run: (c) => c.setHorizontalRule() },
  undo: { run: (c) => c.undo(), enabled: (e) => e.can().undo() },
  redo: { run: (c) => c.redo(), enabled: (e) => e.can().redo() },
};

function heading(level) {
  return {
    run: (c) => c.toggleHeading({ level }),
    active: (e) => e.isActive('heading', { level }),
  };
}

// Asks for the link's URL: an empty answer removes the link, a cancel changes nothing.
function link(chain, editor, win) {
  const current = editor.getAttributes('link').href || '';
  const href = win.prompt('Link URL', current);
  if (href === null) return chain;
  const range = chain.extendMarkRange('link');
  return href.trim() ? range.setLink({ href: href.trim() }) : range.unsetLink();
}

// The StarterKit options for the tools in `tools`: every mark and node no tool produces is off.
function starterKit(tools) {
  const on = (tool, options = {}) => (tools.has(tool) ? options : false);
  const levels = [1, 2, 3].filter((level) => tools.has(`heading-${level}`));
  const lists = tools.has('bullet-list') || tools.has('ordered-list') ? {} : false;
  return {
    bold: on('bold'),
    italic: on('italic'),
    underline: on('underline'),
    strike: on('strike'),
    code: on('code'),
    link: on('link', {
      openOnClick: false,
      defaultProtocol: 'https',
      HTMLAttributes: { target: null, rel: 'noopener noreferrer nofollow' },
    }),
    heading: levels.length ? { levels } : false,
    bulletList: on('bullet-list'),
    orderedList: on('ordered-list'),
    listItem: lists,
    listKeymap: lists,
    blockquote: on('blockquote'),
    codeBlock: on('code-block'),
    horizontalRule: on('horizontal-rule'),
  };
}

// The tools a control's toolbar declares.
function toolsOf(root) {
  return new Set((root.getAttribute('data-rich-text-tools') || '').split(' ').filter(Boolean));
}

// The editable element's accessible name, description and state, from the textarea it replaces.
function editorAttributes(source) {
  const attributes = { role: 'textbox', 'aria-multiline': 'true' };
  const label = source.labels && source.labels[0];
  if (label) {
    if (!label.id) label.id = `${source.id}-label`;
    attributes['aria-labelledby'] = label.id;
  }
  for (const name of ['aria-describedby', 'aria-invalid', 'aria-required']) {
    const value = source.getAttribute(name);
    if (value !== null) attributes[name] = value;
  }
  return attributes;
}

// Presses, enables and disables `toolbar`'s buttons to match the selection in `editor`.
function syncToolbar(toolbar, editor) {
  if (!toolbar || !editor.isEditable) return;
  for (const button of toolbar.querySelectorAll('[data-rich-text-tool]')) {
    const tool = TOOLS[button.getAttribute('data-rich-text-tool')];
    if (!tool) continue;
    if (tool.active) button.setAttribute('aria-pressed', String(tool.active(editor)));
    if (tool.enabled) button.disabled = !tool.enabled(editor);
  }
}

// Mounts an editor from the TipTap module `tiptap` over the control `root`, once.
function mount(root, tiptap) {
  if (root.richTextEditor) return root.richTextEditor;
  const source = root.querySelector('[data-rich-text-source]');
  const content = root.querySelector('[data-rich-text-content]');
  const toolbar = root.querySelector('[data-rich-text-toolbar]');
  if (!source || !content) return null;
  const extensions = [tiptap.StarterKit.configure(starterKit(toolsOf(root)))];
  const placeholder = source.getAttribute('placeholder');
  if (placeholder) extensions.push(tiptap.Placeholder.configure({ placeholder }));
  const editor = new tiptap.Editor({
    element: content,
    content: source.value,
    editable: !source.disabled,
    extensions,
    editorProps: { attributes: editorAttributes(source) },
    // An empty document posts nothing, so the field's blank answer applies.
    onUpdate: ({ editor }) => {
      source.value = editor.isEmpty ? '' : editor.getHTML();
    },
    onTransaction: ({ editor }) => syncToolbar(toolbar, editor),
  });
  // A hidden required control would block the submit with no message to show; the server
  // refuses a blank answer the field has none of instead.
  source.required = false;
  source.hidden = true;
  content.hidden = false;
  if (toolbar) {
    toolbar.hidden = false;
    const first = toolbar.querySelector('[data-rich-text-tool]');
    if (first) first.tabIndex = 0;
    syncToolbar(toolbar, editor);
  }
  root.richTextEditor = editor;
  return editor;
}

// Runs the tool `button` names on its control's editor.
function press(button, win) {
  const root = button.closest('[data-rich-text]');
  const editor = root && root.richTextEditor;
  const tool = TOOLS[button.getAttribute('data-rich-text-tool')];
  if (!editor || !tool) return false;
  return tool.run(editor.chain().focus(), editor, win).run();
}

// Moves the focus along the toolbar holding `button` for an arrow, Home or End key, keeping one
// tab stop on the button last focused. Answers whether the key moved it.
function rove(button, key) {
  const toolbar = button.closest('[data-rich-text-toolbar]');
  if (!toolbar) return false;
  const buttons = Array.from(toolbar.querySelectorAll('[data-rich-text-tool]')).filter(
    (candidate) => !candidate.disabled,
  );
  const at = buttons.indexOf(button);
  const next = {
    ArrowRight: buttons[(at + 1) % buttons.length],
    ArrowLeft: buttons[(at - 1 + buttons.length) % buttons.length],
    Home: buttons[0],
    End: buttons[buttons.length - 1],
  }[key];
  if (!next || at < 0) return false;
  button.tabIndex = -1;
  next.tabIndex = 0;
  next.focus();
  return true;
}

// The TipTap module at `url`, imported once per page.
const modules = new Map();
function load(url) {
  if (!modules.has(url)) modules.set(url, import(url));
  return modules.get(url);
}

// Mounts an editor over each control in `doc` that has none, and destroys the editors of
// controls no longer in the page.
const mounted = new Set();
function scan(doc) {
  for (const root of mounted) {
    if (root.isConnected) continue;
    root.richTextEditor.destroy();
    mounted.delete(root);
  }
  const meta = doc.querySelector('meta[name="tablo-tiptap"]');
  const url = meta && meta.getAttribute('content');
  if (!url) return;
  for (const root of doc.querySelectorAll('[data-rich-text]')) {
    if (root.richTextEditor) continue;
    load(url).then((tiptap) => {
      if (mount(root, tiptap)) mounted.add(root);
    });
  }
}

// Document-level delegation: runtime navigation replaces the page without running this script
// again, and a repeater adds a row's control after load.
function install(doc, win) {
  doc.addEventListener('mousedown', (e) => {
    // Keeps the editor's selection while a tool's button is pressed.
    if (e.target.closest && e.target.closest('[data-rich-text-tool]')) e.preventDefault();
  });
  doc.addEventListener('click', (e) => {
    const button = e.target.closest && e.target.closest('[data-rich-text-tool]');
    if (button) press(button, win);
  });
  doc.addEventListener('keydown', (e) => {
    const button = e.target.closest && e.target.closest('[data-rich-text-tool]');
    if (button && rove(button, e.key)) e.preventDefault();
  });
  scan(doc);
  if (typeof MutationObserver !== 'undefined' && doc.documentElement) {
    new MutationObserver(() => scan(doc)).observe(doc.documentElement, {
      childList: true,
      subtree: true,
    });
  }
}

if (typeof document !== 'undefined') {
  install(document, window);
}

// Exposed for the Node unit test (`rich_text.test.js`); the guard keeps the export out of the
// browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { install, mount, press, rove, starterKit };
}
