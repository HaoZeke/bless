#[cfg(feature = "serve")]
fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("schema")
        .file("schema/bless_log.capnp")
        .run()
        .expect("capnp schema compilation failed");
}

#[cfg(not(feature = "serve"))]
fn main() {}
