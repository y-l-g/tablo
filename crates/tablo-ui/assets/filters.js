// Typed filter controls for Tablo tables.
//
// Each control is a real form field named `f.<name>` (with `data-filter-name`
// carrying `<name>`), so the filter form is an ordinary GET form: a static table
// submits it on change, and the server reads one `?f.<name>=` parameter per
// active filter. The "All" option is `value=""`, which is no filter.
//
// A form marked `data-filters-live` belongs to a live table. Its hidden
// transport (`data-filters-transport`) is bound to the table's `query` signal,
// so the script rewrites that query instead of submitting: it replaces the
// table's filter parameters with the controls' current values, drops the
// cursor (a new filter is a new result set), keeps every other parameter, and
// dispatches a bubbling `change`, which the runtime turns into a signal
// write. The shard re-renders the table in place, with no navigation and no
// scroll jump. A "Clear filters" link (`data-filters-clear`: the bar's own,
// or the empty table's) clears the controls and the query's filters the same
// way, so the controls never show a filter the query dropped. Its `href` is
// the no-JS fallback; a page without a live filter form follows it.
//
// A relation table's parameters carry the relation's prefix (`comments.f.*`),
// spelled on the form as `data-query-prefix`; a page-owned list has bare
// parameters and no prefix. Each relation renders its bar inside its own
// `section[data-relation]`, so a clear link clears the bar of its own section
// rather than the first bar on the page.
//
// Document-level delegation (like bulk.js) so streamed/shard swaps that
// replace table markup need no re-installation.
(() => {
// The table's query with its filter parameters replaced by `filters` (pairs
// of name and value; a blank value is no filter) and the cursor dropped. The
// retired `filters` parameter goes with them.
function withFilters(query, filters, prefix) {
  const params = new URLSearchParams(query);
  prefix = prefix || '';
  for (const key of [...params.keys()]) {
    if (key.startsWith(`${prefix}f.`) || key === `${prefix}filters`) params.delete(key);
  }
  for (const [name, value] of filters) {
    const trimmed = (value || '').trim();
    if (name && trimmed) params.append(`${prefix}f.${name}`, trimmed);
  }
  params.delete(`${prefix}after`);
  params.delete(`${prefix}before`);
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
  transport.value = withFilters(
    transport.value,
    filters,
    form.getAttribute('data-query-prefix') || '',
  );
  transport.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
}

// A click the page may handle in place: a modified click opens the link's
// `href` the browser's way.
function isPlainClick(e) {
  return (
    e.button === 0 && !e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey && !e.defaultPrevented
  );
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
    if (!isPlainClick(e)) return;
    const link = e.target.closest('[data-filters-clear]');
    if (!link) return;
    // The empty table's link sits outside the bar: the table's own section
    // first, so one relation's clear never resets another's bar, then the
    // bar the link sits in, then the page's single live bar.
    const scope = link.closest('[data-relation]');
    const form =
      link.closest('form[data-filters-live]') ||
      (scope && scope.querySelector('form[data-filters-live]')) ||
      document.querySelector('form[data-filters-live]');
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
