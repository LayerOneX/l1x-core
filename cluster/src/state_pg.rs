use anyhow::Error;
use async_trait::async_trait;
use db::postgres::{
	pg_models::Cluster,
	postgres::{PgConnectionType, PostgresDBConn},
};
use db_traits::{base::BaseState, cluster::ClusterState};

use diesel::{self, prelude::*};
use primitives::Address;
pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<Address> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, address: &Address) -> Result<(), Error> {
		// Implementation for create method
		self.store_cluster_address(address).await?;
		Ok(())
	}

	async fn update(&self, _address: &Address) -> Result<(), Error> {
		// Implementation for update method
		Ok(())
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::sql_query(query).execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) =>
				diesel::sql_query(query).execute(&mut *conn.lock().await),
		}?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		Ok(())
	}
}
#[async_trait]
impl<'a> ClusterState for StatePg<'a> {
	async fn store_cluster_address(&self, _cluster_address: &Address) -> Result<(), Error> {
		use db::postgres::schema::cluster::dsl::*;
		let encoded_addr = hex::encode(_cluster_address);
		let cluster_addr = Cluster { cluster_address: encoded_addr };

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) =>
				diesel::insert_into(cluster).values(cluster_addr).execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(cluster)
				.values(cluster_addr)
				.execute(&mut *conn.lock().await),
		}?;

		Ok(())
	}

	async fn store_cluster_addresses(&self, cluster_addresses: &Vec<Address>) -> Result<(), Error> {
		for cluster_address in cluster_addresses {
			self.store_cluster_address(cluster_address).await?;
		}
		Ok(())
	}

	async fn load_all_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		// Implementation for set_schema_version method
		use db::postgres::schema::cluster::dsl::*;

		let mut cluster_addresses = Vec::new();
		let result = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => cluster
				.select(cluster_address)
				.order(cluster_address.desc())
				.load::<String>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => cluster
				.select(cluster_address)
				.order(cluster_address.desc())
				.load::<String>(&mut *conn.lock().await),
		};
		match result {
			Ok(query_res) =>
				for response in query_res {
					let mut cluster_add: Address = [0; 20];
					let addr = hex::decode(response.clone())?;
					if addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&addr);
						cluster_add = array;
					}

					cluster_addresses.push(cluster_add);
				},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
		if cluster_addresses.is_empty() {
			Ok(None)
		} else {
			Ok(Some(cluster_addresses))
		}
	}
}
