use crate::{
	cassandra::DatabaseManager as DatabaseManagerCas,
	postgres::postgres::{PgConnectionType, PostgresDB, PostgresDBConn, PostgresDBPool},
};
use anyhow::{anyhow, Error, Result};

use scylla::Session;
use std::sync::Arc;
use system::{
	config::{Config as SystemConfig, CACHED_CONFIG},
};
use tokio::sync::Mutex;
use system::config::Db;
use system::db_connection_info::DbConnectionInfo;

pub struct Database {
	pub db: DbConnType,
	pub config: SystemConfig,
}
#[derive(Clone)]
pub enum DbTxConn<'a> {
	POSTGRES(PostgresDBConn<'a>),
	CASSANDRA(Arc<Session>),
	ROCKSDB(String),
}

pub enum DbConnType {
	POSTGRES(PostgresDB),
	CASSANDRA(Arc<Session>),
	ROCKSDB(String),
}

impl Database {
	pub async fn new(config: &SystemConfig) {
		{
			let mut lock = CACHED_CONFIG.write().await;
			*lock = Some(Arc::new(config.clone()));
		}

		match config.clone().db {
			Db::Postgres { host, username, password, pool_size, db_name, test_db_name: _ } => {
				let db_connection_info = DbConnectionInfo {
					host,
					username,
					password,
					db_name,
					pool_size,
				};
				PostgresDBPool::initialize_from_config(db_connection_info, config.dev_mode)
					.await
					.expect("PG error: initialize_from_config")
			},
			Db::Cassandra { host, username, password, keyspace } => {
				let db = crate::cassandra::DatabaseManager::initialize(host, username, password, keyspace)
					.await
					.expect("error cassandra initialize");
				if !DatabaseManagerCas::is_session_healthy(db.clone()).await {
					DatabaseManagerCas::replace_session().await.unwrap();
				}
			},
			Db::RocksDb { .. } => {}
		}
	}

	pub async fn re_initialize(config: &SystemConfig) {
		{
			let mut lock = CACHED_CONFIG.write().await;
			*lock = Some(Arc::new(config.clone()));
		}

		match config.clone().db {
			Db::Postgres { host, username, password, pool_size, db_name, test_db_name: _ } => {
				let db_connection_info = DbConnectionInfo {
					host,
					username,
					password,
					db_name,
					pool_size,
				};
				PostgresDBPool::re_initialize_from_config(db_connection_info, config.dev_mode)
					.await
					.expect("PG error: re_initialize_from_config")
			},
			Db::Cassandra { host, username, password, keyspace } => {
				let db = crate::cassandra::DatabaseManager::initialize(host, username, password, keyspace)
					.await
					.expect("error cassandra initialize");
				if !DatabaseManagerCas::is_session_healthy(db.clone()).await {
					DatabaseManagerCas::replace_session().await.unwrap();
				}
			},
			Db::RocksDb { .. } => {}
		}
	}

	pub async fn new_test(config: &SystemConfig) {
		{
			let mut lock = CACHED_CONFIG.write().await;
			*lock = Some(Arc::new(config.clone()));
		}

		match config.clone().db {
			Db::Postgres { host, username, password, pool_size, db_name, test_db_name } => {
				let db_connection_info = DbConnectionInfo {
					host,
					username,
					password,
					db_name,
					pool_size,
				};
				PostgresDBPool::initialize_from_config(db_connection_info, config.dev_mode)
					.await
					.expect("PG error: initialize_from_config")
			},
			Db::Cassandra { host, username, password, keyspace } => {
				let db = crate::cassandra::DatabaseManager::initialize(host, username, password, keyspace)
					.await
					.expect("error cassandra initialize");
				if !DatabaseManagerCas::is_session_healthy(db.clone()).await {
					DatabaseManagerCas::replace_session().await.unwrap();
				}
			},
			Db::RocksDb { .. } => {}
		}
	}

	pub async fn get_connection() -> Result<Database, Error> {
		let config = {
			let lock = CACHED_CONFIG.read().await;
			let config = lock.as_ref().ok_or(anyhow!("get_connection: DB is not initialized!"))?;
			config.clone()
		};

		let db = match config.clone().db.clone() {
			Db::Postgres { host, username, password, pool_size, db_name, test_db_name: _ } => {
				let db_connection_info = DbConnectionInfo {
					host,
					username,
					password,
					db_name,
					pool_size,
				};
				DbConnType::POSTGRES(PostgresDBPool::new_pg_conn_from_config(db_connection_info, config.dev_mode)?)
			},
			Db::Cassandra { host, username, password, keyspace } => DbConnType::CASSANDRA({
				let db = crate::cassandra::DatabaseManager::initialize( host, username, password, keyspace).await?;
				if !DatabaseManagerCas::is_session_healthy(db.clone()).await {
					DatabaseManagerCas::replace_session().await.unwrap();
				}
				db
			}),
			Db::RocksDb { name } => DbConnType::ROCKSDB(crate::rocksdb::DatabaseManager::new(name)),
		};
		Ok(Database {
			db,
			config: <system::config::Config as Clone>::clone(&(*Arc::clone(&config))),
		})
	}

	pub async fn get_postgres_connection() -> Result<PostgresDB, Error> {
		let config = {
			let lock = CACHED_CONFIG.read().await;
			let config = lock.as_ref().ok_or(anyhow!("get_postgres_connection: DB is not initialized!"))?;
			config.clone()
		};
		if let Db::Postgres { host, username, password, pool_size, db_name, test_db_name: _ } = config.clone().db.clone() {
			let db_connection_info = DbConnectionInfo {
				host,
				username,
				password,
				db_name,
				pool_size,
			};
			PostgresDBPool::new_pg_conn_from_config(db_connection_info, config.dev_mode)
		} else {
			Err(anyhow!("PG error: Invalid db config"))
		}

	}

	pub async fn get_pool_connection<'a>() -> Result<DbTxConn<'a>, Error> {
		let config = {
			let lock = CACHED_CONFIG.read().await;
			let config = lock.as_ref().ok_or(anyhow!("get_pool_connection: DB is not initialized!"))?;
			config.clone()
		};

		let conn: DbTxConn<'a> = match config.clone().db.clone() {
			Db::Postgres { host, username, password, pool_size, db_name, test_db_name: _ } => {
				let db_connection_info = DbConnectionInfo {
					host,
					username,
					password,
					db_name,
					pool_size,
				};
				let pg = PostgresDBPool::new_pool_conn_from_config(db_connection_info, config.dev_mode).await?;
				let conn = PgConnectionType::PgConn(Arc::new(Mutex::new(pg.conn)));
				let p_conn = PostgresDBConn { conn, config: pg.config };
				DbTxConn::POSTGRES(p_conn)
			},
			Db::Cassandra { host, username, password, keyspace } =>  DbTxConn::CASSANDRA({
				let db = crate::cassandra::DatabaseManager::initialize(host, username, password, keyspace).await?;
				if !DatabaseManagerCas::is_session_healthy(db.clone()).await {
					DatabaseManagerCas::replace_session().await.unwrap();
				}
				db
			}),
			Db::RocksDb { name } => DbTxConn::ROCKSDB(crate::rocksdb::DatabaseManager::new(name)),
		};

		Ok(conn)
	}

	pub async fn get_test_connection<'a>() -> Result<DbTxConn<'a>, Error> {
		let config = {
			let lock = CACHED_CONFIG.read().await;
			let config = lock.as_ref().ok_or(anyhow!("get_test_connection: DB is not initialized!"))?;
			config.clone()
		};

		let conn: DbTxConn<'a> = match config.clone().db.clone() {
			Db::Postgres { host, username, password, pool_size, db_name, test_db_name: _ } => {
				let db_connection_info = DbConnectionInfo {
					host,
					username,
					password,
					db_name,
					pool_size,
				};
				let pg = PostgresDBPool::new_pool_conn_from_config(db_connection_info, config.dev_mode).await?;
				let conn = PgConnectionType::PgConn(Arc::new(Mutex::new(pg.conn)));
				let p_conn = PostgresDBConn { conn, config: pg.config };
				DbTxConn::POSTGRES(p_conn)
			},
			Db::Cassandra { host, username, password, keyspace } =>  DbTxConn::CASSANDRA({
				let db = crate::cassandra::DatabaseManager::initialize(host, username, password, keyspace).await?;
				if !DatabaseManagerCas::is_session_healthy(db.clone()).await {
					DatabaseManagerCas::replace_session().await.unwrap();
				}
				db
			}),
			Db::RocksDb { name } => DbTxConn::ROCKSDB(crate::rocksdb::DatabaseManager::new(name)),
		};

		Ok(conn)
	}
}
