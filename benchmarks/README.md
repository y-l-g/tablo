# Benchmarks

Server-rendering performance harness for Tablo, following the methodology of
`tokio-rs/topcoat/benchmarks/` (loopback HTTP/1.1 document requests, `oha` load
generator).

Workload: **list with 50 rows, 2 includes (`author` + `comments`), tenancy set,
`ViewAny` enforced**, measured on the real list path (`TableState::from_cx` →
`TablePage::load` over the tenant-scoped `scoped_query` with the declared `.paginate(50)` →
`render_with_state` → HTML). The raw query-only figure is kept as a labeled diagnostic
alongside it. Budget: **< 40 ms p50** on local SQLite, with an opt-in Postgres leg
(see below). The numbers are UNGATED
(GH #171): the harness prints the budget for reference and never PASS/FAILs on it.

Layout:

```
benchmarks/
  tablo/     Tablo/Topcoat app under test (50-row workload, --bench flag)
  scripts/   bench.sh (oha + in-process bench), verify_parity.sh
  results/   benchmark output (gitignored)
```

`benchmarks/tablo` is an unpublished workspace member: it builds against the workspace
lockfile, and the workspace's test, clippy, MSRV, and udeps gates cover it.

## Running

```sh
# Bench the Tablo list (50 rows, 2 includes) without starting a server:
cargo run --manifest-path benchmarks/tablo/Cargo.toml -- --bench --iterations 100
# The process exits nonzero only on harness errors (connect/load/render failure).

# Postgres leg (opt-in — no local Postgres assumed):
# TABLO_BENCH_POSTGRES_URL=postgresql://toasty:toasty@localhost:5432/toasty \
#   cargo run --manifest-path benchmarks/tablo/Cargo.toml -- --bench
# The URL must name a disposable bench database (the leg resets it, pushes
# schema, and seeds under a fresh tenant each run). Without it, only the
# SQLite leg runs.

# Full bench incl. HTTP leg (requires `oha` for the HTTP leg; timings informational, ungated):
./benchmarks/scripts/bench.sh
# -> benchmarks/results/<timestamp>/results.md + oha JSON

# Self-check (tablo renders 50 rows with their includes):
./benchmarks/scripts/verify_parity.sh
```

`cargo run --manifest-path benchmarks/tablo/Cargo.toml` (no flag) still
starts the Topcoat server at `http://localhost:3000/` for manual inspection.

## What "fast" means

* **Preloading** — `include` for `author` + `comments` (3 operations, not 101).
* **Boundaries** — `Table` is a `Boundary` (`data-boundary="table"`); search/filter/page
  swaps only the table, not the shell.
* **Pagination** — cursor pagination (Toasty appends the PK tie-breaker internally).

Results are written per run under `benchmarks/results/` (gitignored). CI builds and lints the
harness with the workspace; it does not run the benchmark.

## Self-check

`verify_parity.sh` asserts the Tablo list renders the 50 rows (`Post 00..Post 49` with `Author`
includes). See `benchmarks/scripts/verify_parity.sh`.
