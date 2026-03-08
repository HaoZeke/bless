#![warn(clippy::all)]

pub mod cli;
pub mod db;
pub mod error;
pub mod logger;
pub mod runner;
pub mod storage_backends {
    pub mod gzip;
    pub mod mongodb;
}

#[cfg(feature = "serve")]
#[allow(clippy::all, unused_parens)]
pub mod bless_log_capnp {
    include!(concat!(env!("OUT_DIR"), "/bless_log_capnp.rs"));
}
#[cfg(feature = "serve")]
pub mod rpc;
#[cfg(feature = "serve")]
pub mod serve;
