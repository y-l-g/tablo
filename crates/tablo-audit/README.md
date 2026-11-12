# tablo-audit

The audit log for Tablo panels: an `AuditEntry` for each record every write of a panel names,
stored inside the write's transaction, one for each start and stop of impersonation, and the
read-only `AuditLog` resource that lists them to the readers who may see them.

It is built on `tablo-core`'s public API alone: a commit hook, an auth hook and an ordinary
resource. The `tablo` facade re-exports it as `tablo::audit` behind the `audit` feature:

```sh
cargo add tablo --features audit
```

```rust
use tablo::audit::PanelAuditExt;

Panel::new("admin").audit_log(|log| log.policy(when(is_admin)));
```

The `Db` registers `tablo::audit::AuditEntry` beside the app's models. The
[resources chapter](https://y-l.fr/tablo/nightly/guide/resources.html#the-audit-log) covers the
log's rules.
