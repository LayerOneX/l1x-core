use anyhow::{anyhow, Error, Result};

use log::info;
use scylla::{authentication::PlainTextAuthenticator, Session, SessionBuilder};
use std::sync::Arc;
use tokio::sync::Mutex;
use system::config::Db;

#[derive(Debug)]
pub struct DatabaseManager {
	session: Arc<Session>,
}

lazy_static::lazy_static! {
	pub static ref CASSANDRA_INSTANCE: Arc<Mutex<Option<DatabaseManager>>> = Arc::new(Mutex::new(None));
	// static ref REPLICATION_ENABLED: bool = env::var("REPLICATION_ENABLED")
	// 	.ok()
	// 	.map(|val| val.eq_ignore_ascii_case("true"))
	// 	.unwrap_or(false);
}

impl DatabaseManager {
	pub async fn new(
		host: String,
		username: String,
		password: String,
		keyspace: String,
	) -> Result<Self, Error> {
		let auth_provider = PlainTextAuthenticator::new(username, password);
		let session = SessionBuilder::new()
			.authenticator_provider(Arc::new(auth_provider))
			.known_node(host)
			.build()
			.await?;
		let config = system::config::Config::default();
		let replication_enable = config.replication_enable;
		let replication_config = if replication_enable {
			"{'class': 'NetworkTopologyStrategy', 'datacenter1': 2}"
		} else {
			"{'class': 'NetworkTopologyStrategy'}"
		};

		let query = format!(
			"CREATE KEYSPACE IF NOT EXISTS {} WITH replication = {} AND durable_writes = true",
			keyspace, replication_config
		);

		session.query(query, &[]).await?;

		let query_key = format!("USE {} ;", keyspace);
		session.query(query_key, &[]).await?;

		Ok(DatabaseManager { session: Arc::new(session) })
	}

	//get_instance method return the instance of cassandra
	pub async fn get_session() -> Result<Arc<Session>, Error> {
		let lock = CASSANDRA_INSTANCE.lock().await;
		if let Some(instance) = &*lock {
			Ok(instance.session.clone())
		} else {
			Err(anyhow!("Cassandra is not initialized!"))
		}
	}
	/// Initialize method which creates cassandra instance once, by calling new method.
	/// If `None` is passed for `connection_info`, default values will be used.
	pub async fn initialize(host: String, username: String, password: String, keyspace: String) -> Result<Arc<Session>, Error> {
		let mut lock = CASSANDRA_INSTANCE.lock().await;
		// If the instance already exists, simply return it.
		if let Some(instance) = &*lock {
			return Ok(instance.session.clone())
		}

		let instance =
			DatabaseManager::new(host.clone(), username, password, keyspace).await.unwrap();
		info!("Connected to Cassandra host  {:?}", host);

		*lock = Some(instance);

		Ok(lock.as_ref().unwrap().session.clone())
	}
	pub async fn replace_session() -> Result<Arc<Session>, Error> {
		let config = system::config::Config::default();
		if let Db::Cassandra { host, username, password, keyspace } = config.db {
			let instance = DatabaseManager::new(host, username, password, keyspace).await?;

			let mut lock = CASSANDRA_INSTANCE.lock().await;
			*lock = Some(instance);

			Ok(lock.as_ref().unwrap().session.clone())
		} else {
			Err(anyhow!("Invalid db config"))
		}


	}
	pub async fn is_session_healthy(session: Arc<Session>) -> bool {
		session.query("SELECT now() FROM system.local", &[]).await.is_ok()
	}
}
