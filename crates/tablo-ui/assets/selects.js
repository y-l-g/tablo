// Option search for Tablo selects.
//
// A `Select::searchable()` renders an `input[data-options-filter]` and an empty
// `ul[data-options-list]` above its `<select>`, inside
// `[data-select-filterable]`.
//
// The list is the point. Narrowing the native `<select>`'s own options is not
// something a page can show: the primitive opts into `appearance: base-select`,
// whose popup is browser chrome that ignores `option[hidden]`, so a filter over
// the select's own options would be invisible. This script therefore renders
// its own filtered listbox from the select's options, and writes the chosen
// value back onto the select, which stays the form control and the no-JS
// fallback.
//
// * Bounded sets: typing narrows the list by label substring
//   (case-insensitive); the placeholder option always stays.
// * Overflowed relationship sets: the wrapper carries
//   `data-options-server="true"` + `data-options-field="<name>"`. Typing
//   debounces (200ms, abort in-flight) a `GET
//   {parent_list_url}/options?field=&q=` fetch that replaces the `<select>`
//   (or `GET {data-options-url}?field=&q=` when the wrapper names the route,
//   as an action's input does: `{url}/-/actions/{name}/options`)
//   options with server markup, preserving the current selection and the
//   placeholder; the list re-renders from the replaced options. The hint
//   ("Too many options — type to search") is server-rendered and stays
//   visible.
//   Without JS the input is inert and the plain select keeps working (stored
//   value kept, relation cannot be changed past the cap).
// * Dependent choices: the wrapper carries `data-options-parent="<name>"`, the
//   field whose value narrows the options, and `data-options-parent-value`.
//   A `change` of that field in the same form fetches `GET
//   {parent_list_url}/options?field=&parent=` and replaces the options, keeping
//   the current choice only when the answer offers it; a blank parent offers
//   none, with no fetch. A choice the answer drops announces its own `change`,
//   so a choice depending on it, or a condition watching it, follows. A search
//   sends the parent value too. Without JS the options stay those of the
//   parent value the page rendered with, and the server refuses a choice of
//   another one.
//
// * Multiple choices: a `ChoiceField::multiple().searchable()` renders an
//   `input[data-choices-filter]` above its checkboxes, inside `[data-choices]`,
//   each box in a `[data-choice]` row. Typing hides the rows whose label does
//   not hold the needle (case-insensitive). Unlike a select's options, the rows
//   are page elements, so hiding them shows; a hidden box that is checked still
//   posts. Enter in the filter submits nothing.
//
// The server renders both controls so the field works without this script;
// with it, the native `<select>` is hidden once the combobox over it is wired.
//
// Document-level delegation, so markup a rerun morphs in needs no
// re-installation.
//
// A script rather than Topcoat handlers: fetching options as the user types,
// or when a parent field changes, would otherwise rerun the page, and a rerun
// clears the fields the user has typed into (tokio-rs/topcoat#504).

const serverTimers = new WeakMap();
const serverControllers = new WeakMap();

// The options route of the choice in `wrap`: the one the server names, an
// action's input route, or else the resource's own, derived from the page.
function optionsBase(wrap) {
  const named = wrap.getAttribute('data-options-url');
  if (named) return named;
  const path = window.location.pathname.replace(/\/$/, '');
  // /admin/posts/create -> /admin/posts ; /admin/posts/<id>/edit -> /admin/posts
  const base = path
    .replace(/\/create$/, '')
    .replace(/\/[^\/]+\/edit$/, '');
  return `${base}/options`;
}

function escapeAttr(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// The `<option>` a server answer has to carry for the current value, or null
// when the answer already echoes it (or nothing is selected). The label is the
// one the option showed before the swap: the answer covers the
// needle, not the selection, and the value is a primary key, so using it as the
// label would show a raw UUID. The label is escaped: it comes from a record.
function preservedOption(current, label, html) {
  if (current === '') return null;
  const escCurrent = escapeAttr(current);
  if (html.includes(`value="${escCurrent}"`)) return null;
  return `<option value="${escCurrent}" selected>${escapeAttr(label)}</option>`;
}

// The `&parent=` a dependent choice's fetch carries, or nothing for an
// independent one.
function parentParam(wrap) {
  if (!wrap.hasAttribute('data-options-parent')) return '';
  return `&parent=${encodeURIComponent(wrap.getAttribute('data-options-parent-value') || '')}`;
}

// The value a changed control posts, as the server reads it: a checkbox posts
// `true` when checked and its hidden `false` otherwise.
function postedValue(control) {
  if (control.type === 'checkbox') return control.checked ? 'true' : 'false';
  return (control.value || '').trim();
}

const dependentControllers = new WeakMap();

// Replace a dependent choice's options with those `parentValue` offers.
async function refreshDependent(wrap, parentValue) {
  const select = wrap.querySelector('select');
  const field = wrap.getAttribute('data-options-field');
  if (!select || !field) return;
  wrap.setAttribute('data-options-parent-value', parentValue);
  const prev = dependentControllers.get(wrap);
  if (prev) prev.abort();
  // A search still pending for the old parent value would land after this
  // refresh and bring its options back.
  const filter = wrap.querySelector('[data-options-filter]');
  if (filter) {
    const timer = serverTimers.get(filter);
    if (timer) clearTimeout(timer);
    serverTimers.delete(filter);
    const search = serverControllers.get(filter);
    if (search) search.abort();
    serverControllers.delete(filter);
    delete wrap.dataset.optionsSearching;
  }
  let html = '';
  let overflow = false;
  if (parentValue !== '') {
    const controller = new AbortController();
    dependentControllers.set(wrap, controller);
    const url = `${optionsBase(wrap)}?field=${encodeURIComponent(field)}${parentParam(wrap)}`;
    try {
      const res = await fetch(url, {
        headers: { Accept: 'text/html' },
        signal: controller.signal,
      });
      // A failed answer offers nothing rather than the old parent's options,
      // which the server would refuse.
      if (res.ok) {
        overflow = res.headers.get('x-options-overflow') === 'true';
        html = await res.text();
      }
    } catch (err) {
      if (err && err.name === 'AbortError') return;
    } finally {
      if (dependentControllers.get(wrap) === controller) {
        dependentControllers.delete(wrap);
      }
    }
  } else {
    dependentControllers.delete(wrap);
  }
  // Past the cap a searchable choice searches the server, as one rendered
  // past it does; a choice with no filter offers nothing to type into, so it
  // offers no option rather than the server's "keep typing" row.
  if (overflow && !filter) html = '';
  const current = select.value;
  const placeholder = select.querySelector('option[value=""]');
  const placeholderHtml = placeholder ? placeholder.outerHTML : '<option value="">-- Select --</option>';
  select.innerHTML = `${placeholderHtml}${html}`;
  const kept = current !== '' && optionFor(select, current) !== null;
  select.value = kept ? current : '';
  if (overflow && filter) {
    wrap.setAttribute('data-options-server', 'true');
  } else {
    wrap.removeAttribute('data-options-server');
  }
  const hint = wrap.querySelector('[data-options-hint]');
  if (hint) hint.hidden = !(overflow && filter);
  if (filter && !kept) filter.value = '';
  if (!kept && current !== '') {
    select.dispatchEvent(new Event('change', { bubbles: true }));
  }
}

// Refresh every choice in `control`'s form that depends on it.
function refreshDependents(control) {
  if (!control || !control.name || !control.closest) return;
  const scope = control.form || control.closest('form') || document;
  scope.querySelectorAll('[data-options-parent]').forEach((wrap) => {
    if (wrap.getAttribute('data-options-parent') !== control.name) return;
    if (wrap.contains(control)) return;
    refreshDependent(wrap, postedValue(control));
  });
}

async function serverSearch(filter, wrap, select, field, needle) {
  const prev = serverControllers.get(filter);
  if (prev) prev.abort();
  const controller = new AbortController();
  serverControllers.set(filter, controller);
  const current = select.value;
  const url = `${optionsBase(wrap)}?field=${encodeURIComponent(field)}&q=${encodeURIComponent(needle)}${parentParam(wrap)}`;
  let html;
  try {
    const res = await fetch(url, {
      headers: { Accept: 'text/html' },
      signal: controller.signal,
    });
    if (!res.ok) return;
    html = await res.text();
  } catch (err) {
    if (err && err.name === 'AbortError') return;
    return;
  } finally {
    if (serverControllers.get(filter) === controller) {
      serverControllers.delete(filter);
    }
  }
  const placeholder = select.querySelector('option[value=""]');
  const placeholderHtml = placeholder ? placeholder.outerHTML : '<option value="">-- Select --</option>';
  // Preserve the current selection across swaps (D2): the server answers the
  // needle and never echoes the current record, so re-attach it when absent.
  // Its label is only in the option the swap is about to drop: the
  // PK is the value, never the thing to show.
  const currentOption = optionFor(select, current);
  const currentLabel = currentOption
    ? (currentOption.textContent || '').trim()
    : current;
  const preserved = preservedOption(current, currentLabel, html) || '';
  const escCurrent = escapeAttr(current);
  const hasCurrent = current !== '' && html.includes(`value="${escCurrent}"`);
  // Mark the fetched current as selected when it matches (string replace,
  // no regex: PKs are opaque strings).
  if (hasCurrent) {
    html = html.split(`value="${escCurrent}"`).join(`value="${escCurrent}" selected`);
  }
  select.innerHTML = `${placeholderHtml}${preserved}${html}`;
}

// --- the visible listbox -----------------------------------------------------

// How many rows the listbox renders at once. It is an affordance, not the
// source of truth: the server owns the real bound, so this only
// keeps the DOM small.
const MAX_LIST_ITEMS = 50;

// Which options a needle offers, as plain data — the one pure decision this
// script makes, split out so it can be unit-tested without a DOM.
//
// An option with an empty `value` is the placeholder ("-- Select --"): it is
// offered whenever the needle is empty (the way back to "no choice") and is
// never counted against the cap, so the cap always buys `limit` real choices.
function matchingOptions(options, needle, limit) {
  const lowered = needle.trim().toLowerCase();
  const rows = [];
  let choices = 0;
  for (const opt of options) {
    if (opt.value === '') {
      if (lowered === '') rows.push({ ...opt });
      continue;
    }
    if (lowered !== '' && !opt.label.toLowerCase().includes(lowered)) continue;
    if (choices >= limit) break;
    choices += 1;
    rows.push({ ...opt });
  }
  return rows;
}

// The parts of the combobox, resolved from either the filter or the list.
function partsOf(node) {
  const combo = node.closest('[data-options-combobox]');
  const wrap = node.closest('[data-select-filterable]');
  return {
    combo,
    wrap,
    filter: combo && combo.querySelector('[data-options-filter]'),
    list: combo && combo.querySelector('[data-options-list]'),
    select: wrap && wrap.querySelector('select'),
  };
}

// The `<option>`s worth offering for `needle`, as plain data.
//
// `value` comes from the option's property rather than its attribute: an
// option with no `value` falls back to its text, which is exactly what the
// browser would submit.
function optionRows(select, needle) {
  const options = Array.from(select.querySelectorAll('option')).map((opt) => ({
    // An option with no `value` submits its text, so that is its value here
    // too; only a genuinely empty value is the placeholder.
    value: opt.value,
    label: (opt.textContent || '').trim(),
    selected: opt.selected,
  }));
  return matchingOptions(options, needle, MAX_LIST_ITEMS);
}

function closeList(combo) {
  const list = combo.querySelector('[data-options-list]');
  if (!list) return;
  list.hidden = true;
  list.replaceChildren();
  // The combobox contract: a closed popup is collapsed, and nothing in it is
  // active any more.
  const filter = combo.querySelector('[data-options-filter]');
  setExpanded(filter, false);
  setActiveDescendant(filter, null);
}

function messageRow(parts, text) {
  const { list, filter } = parts;
  const item = document.createElement('li');
  item.className = 'px-2 py-1.5 text-muted-foreground';
  item.textContent = text;
  list.replaceChildren(item);
  list.hidden = false;
  // The popup is showing, but it holds a status line and no option to be
  // active — which is what lets Enter fall through without picking anything.
  setExpanded(filter, true);
  setActiveDescendant(filter, null);
}

// The id a rendered row carries, so `aria-activedescendant` can name the row
// the keyboard is on. Derived from the list's server-rendered id, which is
// unique per field.
function optionRowId(list, index) {
  return `${list.id || 'options-list'}-option-${index}`;
}

// `aria-expanded` mirrors the popup: the filter and the list are one
// combobox, so the input carries the state.
function setExpanded(filter, expanded) {
  if (filter && filter.setAttribute) {
    filter.setAttribute('aria-expanded', expanded ? 'true' : 'false');
  }
}

// `aria-activedescendant` names the row the keyboard is on, or nothing when no
// row can be active (closed, or a status line). Guarded: a partial markup
// without the input keeps working.
function setActiveDescendant(filter, item) {
  if (!filter || !filter.setAttribute) return;
  if (item && item.id) {
    filter.setAttribute('aria-activedescendant', item.id);
  } else if (filter.removeAttribute) {
    filter.removeAttribute('aria-activedescendant');
  }
}

function renderList({ combo, filter, list, select }) {
  if (!combo || !filter || !list || !select) return;
  const needle = filter.value.trim();
  const rows = optionRows(select, needle);
  if (rows.length === 0) {
    // "Nothing matched" is the whole reason this list exists, so it says so
    // instead of leaving an empty box.
    messageRow(
      { combo, filter, list, select },
      needle === '' ? 'No options' : 'No matching options',
    );
    return;
  }
  list.replaceChildren();
  rows.forEach((row, index) => {
    const item = document.createElement('li');
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', row.selected ? 'true' : 'false');
    item.dataset.value = row.value;
    item.id = optionRowId(list, index);
    item.className =
      'cursor-pointer rounded-md px-2 py-1.5 hover:bg-foreground/5'
      + (row.selected ? ' font-medium' : '');
    item.textContent = row.label;
    list.appendChild(item);
  });
  list.hidden = false;
  setExpanded(filter, true);
  setActiveDescendant(filter, activeItem(list));
}

// The option behind a list row, matched on the property (never an escaped
// selector: a PK is opaque and may contain anything).
function optionFor(select, value) {
  return Array.from(select.options).find((opt) => opt.value === value) || null;
}

// Commit a choice: write it onto the select — the form control — and announce
// it, so a value-bound field stays in step.
function chooseOption({ combo, filter, select }, option) {
  if (!option) return;
  select.value = option.value;
  if (filter) filter.value = (option.textContent || '').trim();
  select.dispatchEvent(new Event('change', { bubbles: true }));
  closeList(combo);
}

// The row the keyboard is on, or the first one.
function activeItem(list) {
  return (
    list.querySelector('[role="option"][aria-selected="true"]')
    || list.querySelector('[role="option"]')
  );
}

// --- a multiple choice's filter -----------------------------------------------

// Whether a multiple choice's row labelled `label` shows for `needle`.
function choiceMatches(label, needle) {
  const lowered = needle.trim().toLowerCase();
  return lowered === '' || label.toLowerCase().includes(lowered);
}

// Show only the rows of the multiple choice `field` that `needle` matches.
function filterChoices(field, needle) {
  field.querySelectorAll('[data-choice]').forEach((row) => {
    row.hidden = !choiceMatches((row.textContent || '').trim(), needle);
  });
}

// --- the control the combobox replaces ---------------------------------------

// Whether the script hides the native `<select>` behind its combobox.
//
// All four parts must resolve: a field whose markup is incomplete keeps a
// visible control rather than losing the only one it has. Hiding is display
// only — the select stays in the markup as the submitted value carrier, and
// `partsOf` still resolves it as a descendant of `[data-select-filterable]`.
function shouldHideNativeSelect({ combo, filter, list, select }) {
  return Boolean(combo && filter && list && select);
}

// The element carrying the replaced control: the native `<select>`'s own
// wrapper when the select primitive renders one — the chevron lives there, and
// hiding the `<select>` alone would leave it behind — and the `<select>`
// itself otherwise. Never the field wrapper: the combobox, the overflow hint,
// and the error message live inside it.
function nativeControl({ wrap, select }) {
  const parent = select.parentElement;
  return parent && parent !== wrap ? parent : select;
}

// Hide the control the combobox replaces, and move the field's requiredness
// onto the combobox.
//
// A `display: none` control is still a candidate for constraint validation, and
// one that fails validation cannot take focus, so the browser refuses the
// submit outright ("An invalid form control with name='author_id' is not
// focusable") before the `submit` event fires. The script therefore drops the
// native `required`, which leaves the server's required check enforcing the
// value, and marks the filter input `aria-required`, which is the control the
// user sees.
function hideNativeSelect(parts) {
  if (!shouldHideNativeSelect(parts)) return;
  nativeControl(parts).hidden = true;
  // The box that replaces the control starts on the current option's label
  // without it an edit form renders an empty box over a selected
  // record. A choice writes the same label into the input (`chooseOption`), so
  // the two paths agree; the placeholder is not a choice and leaves it empty.
  if (parts.filter.value === '' && parts.select.value !== '') {
    const current = optionFor(parts.select, parts.select.value);
    if (current) parts.filter.value = (current.textContent || '').trim();
  }
  if (parts.select.required) {
    parts.select.required = false;
    parts.filter.setAttribute('aria-required', 'true');
  }
}

// Hide the control behind every wired combobox in `root`, or in the document.
// A swap replaces the field markup wholesale, so the pass runs again for the
// nodes that arrive later (see `install`).
function applyHiddenSelects(root) {
  (root || document).querySelectorAll('[data-options-filter]').forEach((filter) => {
    hideNativeSelect(partsOf(filter));
  });
}

// --- wiring ------------------------------------------------------------------

// Everything below only makes sense with a document. It lives in a function so
// this file can also be `require`d by its Node unit test (see the export at the
// bottom), which has no DOM: loading the script must not touch one.
function install() {
  document.addEventListener('input', (e) => {
    const choices = e.target.closest('[data-choices-filter]');
    if (choices) {
      const field = choices.closest('[data-choices]');
      if (field) filterChoices(field, choices.value);
      return;
    }
    if (!e.target.closest('[data-options-filter]')) return;
    const parts = partsOf(e.target);
    if (!parts.select) return;
    const needle = parts.filter.value.trim();
    const field = parts.wrap.getAttribute('data-options-field');
    const server = parts.wrap.getAttribute('data-options-server') === 'true';
    if (server && field) {
      messageRow(parts, 'Searching…');
      parts.wrap.dataset.optionsSearching = 'true';
      const prevTimer = serverTimers.get(parts.filter);
      if (prevTimer) clearTimeout(prevTimer);
      const timer = setTimeout(async () => {
        serverTimers.delete(parts.filter);
        await serverSearch(parts.filter, parts.wrap, parts.select, field, needle);
        delete parts.wrap.dataset.optionsSearching;
        renderList(partsOf(parts.filter));
      }, 200);
      serverTimers.set(parts.filter, timer);
      return;
    }
    renderList(parts);
  });

  // Entering the field opens the list, so the filter shows what it is filtering.
  document.addEventListener('focusin', (e) => {
    if (!e.target.closest('[data-options-filter]')) return;
    renderList(partsOf(e.target));
  });

  // Picking a row: `mousedown` + preventDefault so the input keeps focus and the
  // click is not lost to a blur before it lands.
  document.addEventListener('mousedown', (e) => {
    const item = e.target.closest('[data-options-list] [role="option"]');
    if (!item) return;
    e.preventDefault();
    const parts = partsOf(item);
    if (!parts.select) return;
    chooseOption(parts, optionFor(parts.select, item.dataset.value));
  });

  // Arrows move through the list, Enter picks, Escape closes. Focus stays in the
  // input, which is what makes typing-to-narrow continuous. The input is a
  // combobox, not a text box with an implicit submit: Enter while it has focus
  // is the list's, and never the form's — when the list is showing a
  // status line ("Searching…", "No matching options") there is no row to pick,
  // and the keystroke still must not submit the record the reader is editing.
  document.addEventListener('keydown', (e) => {
    // Enter in a multiple choice's filter narrows; it never submits the form.
    if (e.key === 'Enter' && e.target.closest('[data-choices-filter]')) {
      e.preventDefault();
      return;
    }
    if (!e.target.closest('[data-options-filter]')) return;
    const parts = partsOf(e.target);
    if (!parts.select || !parts.list) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      if (!parts.list.hidden) {
        const item = activeItem(parts.list);
        chooseOption(parts, item && optionFor(parts.select, item.dataset.value));
      }
      return;
    }
    if (parts.list.hidden) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        renderList(parts);
      }
      return;
    }
    const items = Array.from(parts.list.querySelectorAll('[role="option"]'));
    if (items.length === 0) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const current = items.indexOf(activeItem(parts.list));
      const step = e.key === 'ArrowDown' ? 1 : -1;
      const next = items[(current + step + items.length) % items.length];
      items.forEach((item) => item.setAttribute('aria-selected', 'false'));
      next.setAttribute('aria-selected', 'true');
      setActiveDescendant(parts.filter, next);
      next.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Escape') {
      closeList(parts.combo);
    }
  });

  // Tab away: close. Captured, because `focusout` does not bubble usefully here.
  document.addEventListener(
    'focusout',
    (e) => {
      const combo = e.target.closest && e.target.closest('[data-options-combobox]');
      if (!combo || combo.contains(e.relatedTarget)) return;
      closeList(combo);
    },
    true,
  );

  // A click outside the field closes its list, so it never outlives the field.
  document.addEventListener('click', (e) => {
    document.querySelectorAll('[data-options-combobox]').forEach((combo) => {
      if (!combo.contains(e.target)) closeList(combo);
    });
  });

  // A field's change refreshes every choice in its form that depends on it.
  //
  // A server fetch replaces the whole option set *after* the list was rendered,
  // so re-render when it lands — but only while the user is still in the field,
  // or an unrelated `change` would pop the list open.
  document.addEventListener(
    'change',
    (e) => {
      refreshDependents(e.target);
      const select = e.target.closest && e.target.closest('[data-select-filterable] select');
      if (!select) return;
      const wrap = select.closest('[data-select-filterable]');
      if (!wrap || wrap.dataset.optionsSearching === 'true') return;
      const parts = partsOf(select);
      if (parts.filter && document.activeElement === parts.filter) {
        renderList(parts);
      }
    },
    true,
  );

  // The hide pass runs on install and again for markup that arrives later: a
  // swap replaces the field without a page load, and no `load` or
  // `DOMContentLoaded` fires for it.
  applyHiddenSelects();
  if (typeof MutationObserver !== 'undefined' && document.documentElement) {
    new MutationObserver((records) => {
      records.forEach((record) => {
        record.addedNodes.forEach((node) => {
          if (node.nodeType === 1) applyHiddenSelects(node);
        });
      });
    }).observe(document.documentElement, { childList: true, subtree: true });
  }
}

if (typeof document !== 'undefined') install();

// Exposed for the Node unit test (`selects.test.js`); the guard keeps the
// export out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    MAX_LIST_ITEMS,
    choiceMatches,
    matchingOptions,
    preservedOption,
    shouldHideNativeSelect,
  };
}
