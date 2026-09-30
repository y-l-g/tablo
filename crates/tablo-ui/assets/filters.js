// Typed filter controls for Tablo tables.
//
// Each control is a real form field named `f.<name>` (with `data-filter-name`
// carrying `<name>`), so the filter form is an ordinary GET form: a static table
// submits it on change, and the server reads one `?f.<name>=` parameter per
// active filter. The "All" option is `value=""`, which is no filter.
//
// A form marked `data-filters-live` belongs to a live table. Its hidden
// transport (`data-filters-transport`) is bound to the table's `query` signal,
// so the script rewrites that query instead of submitting: it replaces every
// `f.*` parameter with the controls' current values, drops the cursor (a new
// filter is a new result set), keeps every other parameter, and dispatches a
// bubbling `change`, which the runtime turns into a signal write. The shard
// re-renders the table in place, with no navigation and no scroll jump. Its
// "Clear filters" link (`data-filters-clear`) clears the controls and the
// query's filters the same way, since its `href` is the page-load URL.
//
// Document-level delegation (like bulk.js) so streamed/shard swaps that
// replace table markup need no re-installation.
(() => {
// The list query with its filter parameters replaced by `filters` (pairs of
// name and value; a blank value is no filter) and the cursor dropped. The
// retired `filters` parameter goes with them.
function withFilters(query, filters) {
  const params = new URLSearchParams(query);
  for (const key of [...params.keys()]) {
    if (key.startsWith('f.') || key === 'filters') params.delete(key);
  }
  for (const [name, value] of filters) {
    const trimmed = (value || '').trim();
    if (name && trimmed) params.append(`f.${name}`, trimmed);
  }
  params.delete('after');
  params.delete('before');
  return params.toString();
}

function controlValues(form) {
  return [...form.querySelectorAll('[data-filter-name]')].map((el) => [
    el.getAttribute('data-filter-name'),
    el.value,
  ]);
}

// Write the live form's filters into its transport; false when the form has
// none.
function writeFilters(form, filters) {
  const transport = form.querySelector('[data-filters-transport]');
  if (!transport) return false;
  transport.value = withFilters(transport.value, filters);
  transport.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
}

if (typeof document !== 'undefined') {
  document.addEventListener('change', (e) => {
    const control = e.target.closest('[data-filter-name]');
    if (!control) return;
    const form = control.closest('form[data-filters-form]');
    if (!form) return;
    if (form.hasAttribute('data-filters-live')) {
      writeFilters(form, controlValues(form));
      return;
    }
    if (typeof form.requestSubmit === 'function') {
      form.requestSubmit();
    } else {
      form.submit();
    }
  });
  document.addEventListener('click', (e) => {
    const link = e.target.closest('[data-filters-clear]');
    if (!link) return;
    const form = link.closest('form[data-filters-live]');
    if (!form || !writeFilters(form, [])) return;
    e.preventDefault();
    for (const control of form.querySelectorAll('[data-filter-name]')) {
      control.value = '';
    }
  });
}

// Exposed for the Node unit test (`filters.test.js`); inert in the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { withFilters };
}
})();
