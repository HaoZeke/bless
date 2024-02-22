use async_trait::async_trait;
use tokio::io;

#[async_trait]
pub trait Storage {
    async fn save(&self, label: &str, uuid: &str, data: &[String]) -> io::Result<()>;
    async fn finish(&self) -> io::Result<()>;
}
