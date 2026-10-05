// Which embedded variant's fields a form shows.
//
// An embedded enum's derived form renders a variant `Select`
// (`[data-variant-select]`, carrying the discriminant column) and **every**
// variant's payload, each in a `Group` marked with `data-variant-of` (the same
// column) and `data-variant` (the value that variant stores — exactly the
// value the select offers for it as an option).
//
// The server renders all of them and parses whichever variant the select
// names, so this script is display only: it hides the groups whose variant is
// not the chosen one, and leaves every control enabled so the submission still
// carries the payload the server reads. With JavaScript off nothing is hidden —
// every variant's controls visible — so no field the server still accepts is
// lost.
//
// An empty select (a create form: no stored variant to hydrate) shows no
// variant's group until one is chosen, and the server keeps its own payload
// fallback for a submission that names none.
//
// Document-level delegation (like bulk.js) so streamed/shard swaps that
// replace form markup need no re-installation.

// Show the groups of `select`'s enum that carry its value, hide the rest.
function applyVariant(select) {
  const owner = select.getAttribute('data-variant-select');
  if (!owner) return;
  // The form is the scope: two embedded enums in one document — or one enum in
  // two forms — never toggle each other's groups. `data-variant-of` carries the
  // enum's identity, so a value two enums share ("1") is not enough to match.
  const root = select.form || document;
  root.querySelectorAll('[data-variant-of]').forEach((group) => {
    if (group.getAttribute('data-variant-of') !== owner) return;
    const hidden = group.getAttribute('data-variant') !== select.value;
    // Write only a change: the observer below watches `hidden`, so an
    // unconditional write would wake it again.
    if (group.hidden !== hidden) group.hidden = hidden;
  });
}

// Every driver in `root`, for the initial state: an edit form hydrates the
// stored variant server-side, so which group shows is this script's decision.
function applyVariants(root) {
  (root || document).querySelectorAll('[data-variant-select]').forEach(applyVariant);
}

function install() {
  document.addEventListener('change', (event) => {
    const target = event.target;
    const select = target.closest && target.closest('[data-variant-select]');
    if (select) applyVariant(select);
  });
  // Every asset is `defer`red, so the markup is parsed by the time
  // this runs.
  applyVariants();
  // Runtime navigation morphs the next page into this one without running
  // this script again: it adds a form's markup, or reuses the old nodes and
  // drops the `hidden` this script set, or moves a select's `selected`
  // option. Re-apply after each; the write-on-change above makes it settle.
  if (typeof MutationObserver !== 'undefined' && document.documentElement) {
    new MutationObserver(() => applyVariants()).observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['hidden', 'selected'],
    });
  }
}

if (typeof document !== 'undefined') install();
