# 0027 A filter's value is a list of values and a set of named parts

A filter reads a `FilterValue`, which the URL spells with form-encoding's own repetition: each
`f.<name>=` occurrence adds a value, in order, and `f.<name>.<part>=` sets the part `<part>`. A
multiple choice repeats the parameter, a range submits its ends as parts (`f.created.from=`,
`f.created.to=`), and a GET form or `FormData` produces both spellings with no client code. The
state parses the query without knowing the table's filters, so the spelling alone decides the
shape, and each filter decides what it accepts: `single()` is a single-choice filter's one value,
and anything else it rejects is shown as an ignored filter rather than read in part. A filter's
name holds no `.`, so a parameter splits into name and part at its first `.`.

## Rejected

- One delimited value (`f.status=draft,review`, `f.created=2024-01-01..2024-01-31`): a value
  holding the delimiter needs escaping, and a form cannot submit two controls as one value.
- An enum of shapes (one, many, range) chosen at parse time: a multiple choice with one value
  selected parses as one, so every filter would accept two shapes for the same thing.
- Bracketed parts (`f.created[from]=`): a second syntax next to the `.` the table prefix already
  uses.
- First-occurrence-wins for filters, as for the other parameters: it cannot carry a multiple
  choice.
