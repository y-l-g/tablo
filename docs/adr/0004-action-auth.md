# 0004 Mutations are transactional and per-record

Every mutation runs in a framework-owned transaction. The handler loads the target through
`scoped_query` (ADR-0002) and checks the policy on the loaded record inside the transaction,
never on the passed id alone; a bulk write checks every record. Relationship keys are re-checked
against the related resource inside the same transaction. A custom `Action` runs the same way,
with `can_run` per record.

`Resource::after_commit` runs once per committed write, after the commit and before the
response: the place for side effects that must not survive a rollback. A failing hook logs and
does not roll back.
