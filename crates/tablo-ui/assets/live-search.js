// Keystroke-live search debounce for Tablo tables.
//
// The live search input (`data-live-search-input`) is deliberately unbound:
// typing stays local until it pauses, so a burst like "published" triggers
// one table reload instead of nine. After `data-debounce-ms` milliseconds of
// quiet the script rewrites the list query held by the hidden transport
// (`data-live-search-transport`, bound to the table's `query` signal): it sets
// `q`, drops the cursor (a new term is a new result set), keeps every other
// parameter, and dispatches a bubbling `change`, which the runtime turns into a
// signal write. The shard re-renders the table in place, and the runtime's
// abort-in-flight coalescing still applies to the resulting rerun. Pressing
// Enter flushes the pending value immediately instead of waiting out the
// timer. Without JS the `<noscript>` GET form is the search path and this
// script never runs.
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

// The list query with `q` set to `term` (dropped when blank) and the cursor
// dropped; every other parameter kept as it stands.
function withSearch(query, term) {
  const params = new URLSearchParams(query);
  const trimmed = term.trim();
  if (trimmed) {
    params.set('q', trimmed);
  } else {
    params.delete('q');
  }
  params.delete('after');
  params.delete('before');
  return params.toString();
}

function flush(input) {
  const transport = transportFor(input);
  if (!transport || !input.isConnected || !transport.isConnected) return;
  const next = withSearch(transport.value, input.value);
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

// Exposed for the Node unit test (`live-search.test.js`); inert in the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { withSearch };
}
})();
