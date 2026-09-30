// Keystroke-live search debounce for Tablo tables.
//
// The live search input (`data-live-search-input`) is deliberately unbound:
// typing stays local until it pauses, so a burst like "published" triggers
// one table reload instead of nine. After `data-debounce-ms` milliseconds of
// quiet the script rewrites the list query held by the hidden transport
// (`data-live-search-transport`, bound to the table's `query` signal): it sets
// the table's search parameter, drops the cursor (a new term is a new result
// set), keeps every other parameter, and dispatches a bubbling `change`,
// which the runtime turns into a signal write. The shard re-renders the table
// in place, and the runtime's abort-in-flight coalescing still applies to the
// resulting rerun. Pressing Enter flushes the pending value immediately
// instead of waiting out the timer. The empty table's "Clear search" link
// (`data-search-clear`) empties the input and flushes, so the input never
// shows a term the query dropped; its `href` is the fallback a page without
// a live input follows. Without JS the `<noscript>` GET form is the search
// path and this script never runs.
//
// A relation table's parameters carry the relation's prefix (`comments.q`),
// spelled on the host as `data-query-prefix`; a page-owned list has bare
// parameters and no prefix. Each relation renders its host inside its own
// `section[data-relation]`, so a clear link clears the input of its own
// section rather than the first input on the page.
//
// Document-level delegation (like filters.js) so streamed/shard swaps that
// replace table markup need no re-installation. Per-input timers live in a
// WeakMap keyed by the visible input; a timer firing for a detached (swapped
// out) input is dropped, so a stale value can never overwrite a newer one.
//
// Wrapped in an IIFE: deferred classic scripts share the global lexical
// environment, and generic names like `flush` must not collide with sibling
// assets (bulk.js, filters.js, selects.js).
(() => {
const timers = new WeakMap();

function transportFor(input) {
  const host = input.closest('[data-live-search]');
  if (!host) return null;
  return host.querySelector('[data-live-search-transport]');
}

function prefixFor(input) {
  const host = input.closest('[data-live-search]');
  return (host && host.getAttribute('data-query-prefix')) || '';
}

// The table's query with its search parameter set to `term` (dropped when
// blank) and the cursor dropped; every other parameter kept as it stands.
function withSearch(query, term, prefix) {
  const params = new URLSearchParams(query);
  prefix = prefix || '';
  const trimmed = term.trim();
  if (trimmed) {
    params.set(`${prefix}q`, trimmed);
  } else {
    params.delete(`${prefix}q`);
  }
  params.delete(`${prefix}after`);
  params.delete(`${prefix}before`);
  return params.toString();
}

function flush(input) {
  const transport = transportFor(input);
  if (!transport || !input.isConnected || !transport.isConnected) return;
  const next = withSearch(transport.value, input.value, prefixFor(input));
  if (transport.value !== next) {
    transport.value = next;
    transport.dispatchEvent(new Event('change', { bubbles: true }));
  }
}

function debounceMs(input) {
  const raw = parseInt(input.getAttribute('data-debounce-ms') || '200', 10);
  return Number.isFinite(raw) && raw >= 0 ? raw : 200;
}

document.addEventListener('input', (e) => {
  const input = e.target.closest('[data-live-search-input]');
  if (!input) return;
  if (!transportFor(input)) return;
  const pending = timers.get(input);
  if (pending) clearTimeout(pending);
  timers.set(
    input,
    setTimeout(() => {
      timers.delete(input);
      flush(input);
    }, debounceMs(input)),
  );
});

// Enter means "search now": flush instead of waiting out the timer.
document.addEventListener('keydown', (e) => {
  if (e.key !== 'Enter') return;
  const input = e.target.closest('[data-live-search-input]');
  if (!input) return;
  const pending = timers.get(input);
  if (pending) {
    clearTimeout(pending);
    timers.delete(input);
  }
  flush(input);
});

// "Clear search" clears the input it names, not only the query. A modified
// click opens the link's `href` the browser's way.
document.addEventListener('click', (e) => {
  if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.defaultPrevented) {
    return;
  }
  const link = e.target.closest('[data-search-clear]');
  if (!link) return;
  // The link sits in the table, the input above it: the table's own section
  // first, so one relation's clear never empties another's input, then the
  // page's single live input.
  const scope = link.closest('[data-relation]');
  const input =
    (scope && scope.querySelector('[data-live-search-input]')) ||
    document.querySelector('[data-live-search-input]');
  if (!input || !transportFor(input)) return;
  e.preventDefault();
  const pending = timers.get(input);
  if (pending) {
    clearTimeout(pending);
    timers.delete(input);
  }
  input.value = '';
  flush(input);
});

// Exposed for the Node unit test (`live-search.test.js`); inert in the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { withSearch };
}
})();
