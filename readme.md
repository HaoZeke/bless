![img](branding/logo/bless_logo.png)


# Table of Contents

-   [About](#orgbe6d539)
    -   [Why?](#org2f008c9)
    -   [Features](#orge7b86ad)
    -   [Design](#orgfd513c6)
-   [Usage](#org0e17b2e)
-   [Development](#org67fe805)
    -   [Component Rationale](#orgeaf99b1)
    -   [Local MongoDB](#org2d5e207)
    -   [Documentation](#org78d2e44)
-   [License](#orgff774ac)


<a id="orgbe6d539"></a>

# About

A simple command line wrapper for repeated runs, with metadata and lightweight
tracking.


<a id="org2f008c9"></a>

## Why?

During development, a full interface to HPC oriented workflow engines like
AiiDA, Fireworks, Jobflow, and the like is typically too heavy, and more
importantly, the API is often not stable. That being said, this could also be
used in conjunction with `pychum` and workflow runners like Snakemake to store
full logs for later.


<a id="orge7b86ad"></a>

## Features

-   Fast, compressed, logging, with almost zero overhead
-   File and MongoDB interface
    -   Including large log support via GridFS
    -   Helper script also provided to recreate logs


<a id="orgfd513c6"></a>

## Design


### File (gzip) Writer

This overrides the `Log` levels of Rust, so:

-   `TRACE` is for additional information for the command, as written by `bless`
-   `INFO` corresponds to `stdout` of the command
-   `ERROR` corresponds to a `bless` error
-   `WARN` corresponds to `stderr` of the command


### MongoDB Writer

Only `stderr` and `stdout` of the command are stored in a `.gz` file which is
added to the database as a binary blob, with additional metadata.


<a id="org0e17b2e"></a>

# Usage

    cargo build --release
    MONGODB_URI="mongodb://localhost:27017/" ./target/release/bless --use-mongodb -- echo "bye"
    # Then view it in mongosh
    # or
    ./target/release/bless -- echo "bye"
    zcat default_label*.gz


<a id="org67fe805"></a>

# Development


<a id="orgeaf99b1"></a>

## Component Rationale

-   **Duct:** For [the gotchas](https://github.com/oconnor663/duct.py/blob/master/gotchas.md)
-   **Wild:** For cross-platform globs
-   **Flate2:** For compression
-   **UUID:** For the unique IDs
-   **Fern:** For log handling


<a id="org2d5e207"></a>

## Local MongoDB

Assuming `pixi` is used to get an instance of `mongod`.

    pixi run mongod --dbpath $(pwd)/data/database
    MONGODB_URI="mongodb://localhost:27017/" ./target/release/bless --use-mongodb -- $CMD_TO_RUN

I use `npx mongosh` for validating commands.

    npx mongsh
    use local
    # Show all entries
    db.commands.find()
    # Suppress blob data
    db.commands.find({}, { gzip_blob: 0 })
    # Dangerous, drop all entries!
    db.getCollectionNames().forEach(c=>db[c].drop())


### Extracting run output

Since the `.gzip` is stored as binary data keyed to the `gzip_blob` entry, a
small helper script is provided.

    python scripts/get_db_gzip.py --db-name local --collection-name commands --query-field label --query-value np_nogrid --output-file output.gzip

We can check that the generated file is the same.

    sha256sum output.gzip np_nogrid_b7b3a733-7383-4367-b5c6-abaf55051114.log.gz
    71b3678846c29daa1c486473f7770b72ff08b99001d26e38df81662e6eeedc3f  output.gzip
    71b3678846c29daa1c486473f7770b72ff08b99001d26e38df81662e6eeedc3f  np_nogrid_b7b3a733-7383-4367-b5c6-abaf55051114.log.gz
    # or
    diff output.gzip np_nogrid*.gz


### Large File Support

When a log is larger than 15 MB, `bless` will use GridFS to chunk and store the
file. The script automatically handles the case of having large log files by
using GridFS to assemble them from `gzip_blob_id`. This can be requested via the
command-line manually as well.


<a id="org78d2e44"></a>

## Documentation


### Readme

The `readme` can be constructed via:

    ./scripts/org_to_md.sh readme_src.org readme.md

metadata more generically.


<a id="orgff774ac"></a>

# License

MIT. However, this is an academic resource, so **please cite** as much as possible
via:

-   The Zenodo DOI for general use.
-   TBD a publication

