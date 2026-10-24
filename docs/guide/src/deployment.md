# Deployment

A deployed panel is one release binary, the asset bundle built with it, and a database whose
schema migrations keep current. [Security](./security.md#your-deployment-must-provide) lists what
the host must provide around them: HTTPS, the client's address behind a proxy, and rate limits.

## Build the binary and its bundle

```sh
topcoat asset bundle --release -p my-app
```

The command builds the release binary and writes its asset bundle to `assets/` beside it, in
`target/release/`. Ship `target/release/my-app` and `target/release/assets/` together: the bundle
holds the stylesheet, the font, Topcoat's runtime and the shell's scripts under names that one
build computed, so a bundle from another build does not match. `AssetBundle::load()` reads the
directory beside the executable; `AssetBundle::load_dir(path)` reads one elsewhere. A package
with several binaries, such as the migration binary below, names the server's with
`--bin my-app`.

The stylesheet is compiled earlier, by `tablo_build::tailwind()` in `build.rs`, which downloads
the Tailwind CLI into Topcoat's cache on its first run: a build machine needs network access once.

## Migrate the database

`db.push_schema()` creates every table from scratch, which suits a prototype and a test, not a
database holding data. In production, generate SQL migrations with Toasty's migration CLI and
apply them before the new binary starts.

`toasty-cli` is a library: the app builds its own small binary around the same `Db` its server
uses, so the migrations cover every model the panel reads, the ones `tablo::models!` adds
included:

```rust,ignore
// src/bin/migrate.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db = toasty::Db::builder()
        .models(tablo::models!(my_app::Post))
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    toasty_cli::ToastyCli::with_config(db, toasty_cli::Config::load()?)
        .parse_and_run()
        .await
}
```

- `cargo run --bin migrate -- migration generate` diffs the models against the last snapshot and
  writes a migration; commit it.
- `target/release/migrate migration apply` runs the pending migrations; run it on each deploy,
  before the server starts.

The [Toasty guide](https://tokio-rs.github.io/toasty/0.11.0/guide/) covers `Toasty.toml`, renames
and the other subcommands. A new database has no user to sign in with: seed the first one with
`auth::create_admin`, from a one-off binary or a startup step that runs when no admin exists.

## Run it

- `topcoat::start` binds `HOST`:`PORT`, `127.0.0.1:3000` by default; in a container set
  `HOST=0.0.0.0`.
- Terminate HTTPS in front of the binary: the panel's cookies are `Secure`, so without it every
  form POST fails its CSRF check.
- Load the router's cookie `Key` from a secret holding at least 64 random bytes (decode a hex or
  base64 secret first: its text has fewer bits than its length), never from
  `Key::generate()` and never committed: it seals two-factor secrets, so a key that changes on
  restart, or differs between replicas, leaves every enrolled account unable to sign in, and a
  key that leaks lets its holder read them.
- Behind a reverse proxy, install `TrustedProxies` on the router, so the sign-in throttle counts
  the client's address and not the proxy's.
- Keep uploads on storage that outlives the binary: the directory `Panel::uploads_dir` writes to
  is a volume, or the `Uploader` writes elsewhere. Its links are signed with a key derived from
  the router's cookie `Key`, so replicas that should open each other's links share that `Key`.
- Several replicas share the database, and sessions with it. The sign-in throttle counts per
  process, so limit attempts across replicas at the proxy.
- The notification bell wakes over an in-process channel, as
  [ADR-0028](https://github.com/y-l-g/tablo/blob/master/docs/adr/0028-stored-notifications.md)
  records: a send, a read or a sign-out on one replica re-renders only the bells connected to that
  replica. A bell connected to another one shows the new state when its page reloads or its own
  replica next wakes it; a session revoked elsewhere stops at that wake, before it shows anything
  new.
