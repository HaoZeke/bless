use crate::rpc::server::BlessServerImpl;
use capnp_rpc::twoparty::VatNetwork;
use capnp_rpc::RpcSystem;
use futures::AsyncReadExt;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio_util::compat::TokioAsyncReadCompatExt;

/// Run the bless serve mode, accepting capnp RPC connections.
pub async fn run_server(addr: &str) -> Result<(), crate::error::BlessError> {
    let data_dir = dirs_or_default();

    let addr = if addr.starts_with(':') {
        format!("0.0.0.0{}", addr)
    } else {
        addr.to_string()
    };

    let listener = TcpListener::bind(&addr).await?;
    eprintln!("[serve] listening on {addr}");
    eprintln!("[serve] data dir: {}", data_dir.display());

    loop {
        let (stream, remote) = listener.accept().await?;
        eprintln!("[serve] connection from {remote}");

        let data_dir = data_dir.clone();
        tokio::task::spawn_local(async move {
            let (reader, writer) = TokioAsyncReadCompatExt::compat(stream).split();
            let network = VatNetwork::new(
                reader,
                writer,
                capnp_rpc::rpc_twoparty_capnp::Side::Server,
                Default::default(),
            );

            let server = BlessServerImpl::new(data_dir);
            let client: crate::bless_log_capnp::bless_server::Client =
                capnp_rpc::new_client(server);

            let rpc = RpcSystem::new(Box::new(network), Some(client.clone().client));
            if let Err(e) = rpc.await {
                eprintln!("[serve] RPC error from {remote}: {e}");
            }
        });
    }
}

fn dirs_or_default() -> PathBuf {
    if let Some(data) = dirs_next() {
        data.join("bless").join("sessions")
    } else {
        PathBuf::from(".bless_sessions")
    }
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("XDG_DATA_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local").join("share"))
        })
}
