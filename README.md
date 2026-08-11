![img](branding/logo/bless_logo.png)

# About

A command line wrapper for repeated runs, with metadata and lightweight
tracking. Wraps any command, captures stdout/stderr with timestamps, writes
compressed gzip logs, and optionally stores results in MongoDB.

## Why?

During development, a full interface to HPC oriented workflow engines like
AiiDA, Fireworks, Jobflow, and the like is typically too heavy, and more
importantly, the API is often not stable. `bless` provides a minimal layer that
records what was run, when, and what it produced, without imposing any workflow
structure. It can also be used alongside `pychum` and workflow runners like
Snakemake to store metadata more generically.

## Design

`bless` uses `tokio::process` for async subprocess execution with concurrent
stdout/stderr streaming. The wrapped command's exit code is passed through via
`ExitCode`, so scripts and CI can inspect the real status. All errors are
represented as `BlessError` (via `thiserror`), covering I/O, MongoDB, and
logger init.

### Log levels

- `TRACE` -- additional metadata written by `bless` (label, uuid, duration)
- `INFO` -- stdout of the wrapped command
- `WARN` -- stderr of the wrapped command
- `ERROR` -- bless-level errors (command failure, I/O errors)

### Output formats

The `--format` flag controls stdout rendering:

- `log` (default) -- `[timestamp LEVEL] message`
- `jsonl` -- one JSON object per line with `ts`, `level`, `msg` fields

The gzip file always uses the timestamped log format regardless of `--format`.

# Installation

From crates.io:

```bash
cargo install bless
```

To include serve mode (capnp RPC log aggregation):

```bash
cargo install bless --features serve
```

From source:

```bash
cargo build --release
# Binary at ./target/release/bless
```

Or install the local checkout:

```bash
cargo install --path .
```

Serve mode from a local checkout:

```bash
cargo install --path . --features serve
```

# Usage

Basic usage, wrapping a build command:

```bash
bless --label myproject -- make -j8
```

This creates `myproject_{uuid}.log.gz` with the full captured output.

Suppress timestamps on stdout (gzip file still has them):

```bash
bless --label myproject --no-timestamp -- make -j8
```

JSONL output for structured log processing:

```bash
bless --label myproject --format jsonl -- pytest -v
```

Custom output path:

```bash
bless --label build -o build_log.gz -- cargo build 2>&1
```

Stdout only (no gzip file):

```bash
bless -o - -- echo "just watching"
```

Split stdout and stderr into separate gzip files:

```bash
bless --label run --split -- ./simulation
# Produces run_{uuid}_stdout.log.gz and run_{uuid}_stderr.log.gz
```

## Serve mode (feature-gated)

When built with `--features serve`, two additional flags are available.

Start a log aggregation server (capnp RPC):

```bash
bless --serve :9000 -- true
```

Stream logs from a run to a remote bless server:

```bash
bless --remote 192.168.1.10:9000 --label worker1 -- ./job.sh
```

Add `--local` to also write a local gzip alongside remote streaming:

```bash
bless --remote 192.168.1.10:9000 --local --label worker1 -- ./job.sh
```

Session data is stored under `$XDG_DATA_HOME/bless/sessions/` (or
`$HOME/.local/share/bless/sessions/`).

## MongoDB

Store run output and metadata in MongoDB:

```bash
MONGODB_URI="mongodb://localhost:27017/" bless --use-mongodb --label experiment -- ./run.sh
```

The gzip file the logger opened (not a reconstructed name), plus command
args, label, uuid, timestamps, and duration, are saved to the `commands`
collection in the `bless` database. Override with `--db` / `--collection`
or `MONGODB_DB` / `MONGODB_COLLECTION`. `--use-mongodb` with `-o -` is an
error: there is no gzip to upload. Every document has `stream`: `""` for
a combined run, `stdout` or `stderr` when `--split` writes two documents
that share `run_uuid`.

Assuming `pixi` is used to get an instance of `mongod`:

```bash
pixi run mongod --dbpath $(pwd)/data/database
MONGODB_URI="mongodb://localhost:27017/" bless --use-mongodb -- $CMD_TO_RUN
```

Inspect results with `npx mongosh`:

```bash
npx mongosh
use bless
# Show all entries
db.commands.find()
# Suppress blob data
db.commands.find({}, { gzip_blob: 0 })
# Drop all entries
db.getCollectionNames().forEach(c=>db[c].drop())
```

### Extracting run output

A helper script writes the stored gzip to a file. Documents with
`storage=gridfs` or `gzip_blob_id` are streamed from GridFS; otherwise the
embedded `gzip_blob` Binary is used:

```bash
python scripts/get_db_gzip.py --db-name bless --collection-name commands --query-field args --query-value orca.inp
```

# Documentation

The docs site can be built with:

```bash
pixi run docbld
```

# License

MIT. However, this is an academic resource, so **please cite** as much as
possible via:

- [10.5281/zenodo.18912525](https://doi.org/10.5281/zenodo.18912525) (concept DOI; v0.2.0 is [10.5281/zenodo.18912526](https://doi.org/10.5281/zenodo.18912526))
