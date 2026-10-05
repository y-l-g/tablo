# Design documents

Transient, guide-level proposals for cross-cutting public-API changes: the contract reviewers
accept before the implementation lands. Routine features and bug fixes do not need one; a change
that reshapes `Panel`, `Resource`, `Table`, `Schema`, or the policy/tenancy seams does.

An ADR records a decision already taken. A design document proposes one. The implementation PR
deletes the design document: the durable reasoning moves to an ADR and the usage to the guide
and rustdoc. Git history keeps the proposal record.

## Workflow

1. Open a feature-proposal issue describing the problem and the shape of the solution.
2. Land the design document under `docs/dev/design/` in its own PR. Reviewers debate the API on
   that PR; it contains no implementation.
3. Land the implementation as a follow-up PR once the design is accepted, deleting the design
   document in the same PR. The accepted design is the contract: review of the implementation
   should not re-litigate decisions made there.

## Starting a new document

Copy [`_template.md`](_template.md) to `docs/dev/design/<feature-name>.md` and fill it in. The
template's sections are the default shape, not a fixed form: keep them in that order where they
fit, delete one that does not apply and say why in one line rather than leaving it empty, and add
a section when the design needs one the template lacks.
