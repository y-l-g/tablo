---
name: prose
description: Always use this skill before writing long-form markdown documentation for Tablo
---

# Prose

Write per [`CONTRIBUTING.md`](../../../CONTRIBUTING.md#prose). It is the authoritative
source for voice, banned words, and structure; the [where-writing-lives
list](../../../CONTRIBUTING.md#decisions-and-vocabulary) is authoritative for placement.
The rules below are the ones agents miss most often.

## Placement

- Vocabulary and domain terms: `CONTEXT.md`.
- Decisions: `docs/adr/`.
- Transient API proposals: `docs/dev/design/`.
- Upstream API freshness: `docs/dev/upstream-notes.md`.
- User guide: `docs/guide/` (mdBook); `README.md` is the short entry point.
- Contributor specs (commits, prose, labels, testing): `docs/dev/`.
- Agent tracker notes: `docs/agents/`.

## Structure

Lead with a code sample where a sample answers the question. Match the file's
existing line wrapping: prose files in this repo wrap near column 100, and
commit subjects stay under 100 characters per `COMMITS.md`.

## General

- Write in plain English. No fancy sentence structure.
- Document the current state only; never reference previous iterations ("this
  used to be A but is now B"). History belongs in a commit message or an ADR.
