use anyhow::Error;
use async_trait::async_trait;

#[async_trait]
pub trait BaseState<U>: Send + Sync {
	async fn create_table(&self) -> Result<(), Error>;
	async fn create(&self, u: &U) -> Result<(), Error>;
	async fn update(&self, u: &U) -> Result<(), Error>;
	async fn raw_query(&self, query: &str) -> Result<(), Error>;
	async fn set_schema_version(&self, version: u32) -> Result<(), Error>;
}
