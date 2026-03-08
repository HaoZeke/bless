bless
=====

CLI logging helper for repeated runs with metadata tracking.

``bless`` wraps any command, captures stdout/stderr with timestamps, and stores
the output as compressed gzip logs. Optionally stores logs in MongoDB or streams
them to a central server via Cap'n Proto RPC.

.. toctree::
   :maxdepth: 2
   :caption: Tutorials

   tutorials/quickstart

.. toctree::
   :maxdepth: 2
   :caption: How-to guides

   howto/cf-ci-integration
   howto/mongodb-setup
   howto/serve-mode

.. toctree::
   :maxdepth: 2
   :caption: Reference

   reference/cli
   reference/log-format
   reference/capnp-schema

.. toctree::
   :maxdepth: 2
   :caption: Development

   contributing/index
   changelog
