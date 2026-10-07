# Benchmarks

Measures the list path: 50 posts with their `author` and `comments` includes, tenancy set, the
`ViewAny` check enforced, rendered to HTML. `benchmarks/tablo` declares the models and resources
and seeds in-memory SQLite. The budget is under 40 ms p50 on SQLite; it is printed for reference
and never gated (GH #171). CI builds and lints the harness but does not run it.

```sh
# In-process bench, no server:
cargo run -p storefront-tablo -- --bench --iterations 100

# Add the Postgres leg; the URL must name a disposable database, which each run resets:
TABLO_BENCH_POSTGRES_URL=postgresql://toasty:toasty@localhost:5432/toasty \
  cargo run -p storefront-tablo -- --bench

# Full run with the HTTP leg (needs `oha`), written to benchmarks/results/<timestamp>/:
./benchmarks/scripts/bench.sh

# Check that the list renders the 50 rows with their includes:
./benchmarks/scripts/verify_parity.sh

# Serve the panel at http://localhost:3000/ for manual inspection:
cargo run -p storefront-tablo
```
