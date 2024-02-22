![img](branding/logo/bless_logo.png)


# Table of Contents

-   [About](#orgf98f51a)
    -   [Why?](#org1cbb517)
-   [Usage](#org2cbc8ca)
-   [Development](#orgbd356c3)
    -   [Component Rationale](#org9a41732)
    -   [Local MongoDB](#orgef90813)
    -   [Documentation](#orgc71b0e2)
-   [License](#orgc7a316b)


<a id="orgf98f51a"></a>

# About

A simple command line wrapper for repeated runs, with metadata and lightweight
tracking.


<a id="org1cbb517"></a>

## Why?

During development, a full interface to HPC oriented workflow engines like
AiiDA, Fireworks, Jobflow, and the like is typically too heavy, and more
importantly, the API is often not stable. That being said, this could also be
used in conjunction with `pychum` and workflow runners like Snakemake to store


<a id="org2cbc8ca"></a>

# Usage

    cargo build --release
    MONGODB_URI="mongodb://localhost:27017/" ./target/release/bless --use-mongodb -- echo "bye"
    # Then view it in mongosh
    # or
    ./target/release/bless -- echo "bye"
    zcat default_label*.gz


<a id="orgbd356c3"></a>

# Development


<a id="org9a41732"></a>

## Component Rationale

-   **Duct:** For [the gotchas](https://github.com/oconnor663/duct.py/blob/master/gotchas.md)
-   **Wild:** For cross-platform globs
-   **Flate2:** For compression
-   **UUID:** For the unique IDs


<a id="orgef90813"></a>

## Local MongoDB

Assuming `pixi` is used to get an instance of `mongod`.

    pixi run mongod --dbpath $(pwd)/data/database
    MONGODB_URI="mongodb://localhost:27017/" ./target/release/bless --use-mongodb -- $CMD_TO_RUN

I use `npx mongosh` for validating commands.

    npx mongsh
    use local
    db.commands.find()


<a id="orgc71b0e2"></a>

## Documentation


### Readme

The `readme` can be constructed via:

    ./scripts/org_to_md.sh readme_src.org readme.md

metadata more generically.


<a id="orgc7a316b"></a>

# License

MIT. However, this is an academic resource, so **please cite** as much as possible
via:

-   The Zenodo DOI for general use.
-   TBD a publication

