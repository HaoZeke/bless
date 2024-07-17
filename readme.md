![img](branding/logo/bless_logo.png)


# Table of Contents

-   [About](#org3bf058b)
    -   [Why?](#org37bc08e)
    -   [Design](#orgfc61a1a)
-   [Usage](#org2b345d0)
-   [Development](#org603159f)
    -   [Component Rationale](#org89150b7)
    -   [Local MongoDB](#orgb4d73ad)
    -   [Documentation](#org7940300)
-   [License](#org654b5a6)


<a id="org3bf058b"></a>

# About

A simple command line wrapper for repeated runs, with metadata and lightweight
tracking.


<a id="org37bc08e"></a>

## Why?

During development, a full interface to HPC oriented workflow engines like
AiiDA, Fireworks, Jobflow, and the like is typically too heavy, and more
importantly, the API is often not stable. That being said, this could also be
used in conjunction with `pychum` and workflow runners like Snakemake to store


<a id="orgfc61a1a"></a>

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


<a id="org2b345d0"></a>

# Usage

    cargo build --release
    MONGODB_URI="mongodb://localhost:27017/" ./target/release/bless --use-mongodb -- echo "bye"
    # Then view it in mongosh
    # or
    ./target/release/bless -- echo "bye"
    zcat default_label*.gz


<a id="org603159f"></a>

# Development


<a id="org89150b7"></a>

## Component Rationale

-   **Duct:** For [the gotchas](https://github.com/oconnor663/duct.py/blob/master/gotchas.md)
-   **Wild:** For cross-platform globs
-   **Flate2:** For compression
-   **UUID:** For the unique IDs
-   **Fern:** For log handling


<a id="orgb4d73ad"></a>

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

Since the `.gzip` is stored as binary data keyed to the entry, a small helper script is provided.

    python scripts/get_db_gzip.py --db-name local --collection-name commands --query-field args --query-value orca.inp


<a id="org7940300"></a>

## Documentation


### Readme

The `readme` can be constructed via:

    ./scripts/org_to_md.sh readme_src.org readme.md

metadata more generically.


<a id="org654b5a6"></a>

# License

MIT. However, this is an academic resource, so **please cite** as much as possible
via:

-   The Zenodo DOI for general use.
-   TBD a publication

