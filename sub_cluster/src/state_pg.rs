use anyhow::Error;
use async_trait::async_trait;

use db::postgres::{
	pg_models::SubCluster,
	postgres::{PgConnectionType, PostgresDBConn},
	schema,
};

use db_traits::{base::BaseState, sub_cluster::SubClusterState};
use diesel::{self, prelude::*};
use primitives::Address;

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}
#[async_trait]
impl<'a> BaseState<SubCluster> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, t: &SubCluster) -> Result<(), Error> {
		let mut sub_cluster_add: Address = [0; 20];
		let vec = hex::decode(t.sub_cluster_address.clone())?;
		if vec.len() == 20 {
			let mut array = [0u8; 20];
			array.copy_from_slice(&vec);
			sub_cluster_add = array;
		}
		self.store_sub_cluster_address(&sub_cluster_add).await?;
		Ok(())
	}

	async fn update(&self, _t: &SubCluster) -> Result<(), Error> {
		// Implementation for update method
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::sql_query(query).execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) =>
				diesel::sql_query(query).execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}
}

#[async_trait]
impl<'a> SubClusterState for StatePg<'a> {
	async fn store_sub_cluster_address(&self, sub_cluster_address: &Address) -> Result<(), Error> {
		let encode_address = hex::encode(sub_cluster_address);
		let new_cluster_address = SubCluster { sub_cluster_address: encode_address };

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(schema::sub_cluster::table)
				.values(new_cluster_address)
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(schema::sub_cluster::table)
				.values(new_cluster_address)
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn store_sub_cluster_addresses(
		&self,
		sub_cluster_addresses: &Vec<Address>,
	) -> Result<(), Error> {
		// Implementation for set_schema_version method
		for sub_cluster_address in sub_cluster_addresses {
			self.store_sub_cluster_address(sub_cluster_address).await?;
		}
		Ok(())
	}

	async fn load_all_sub_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		// Implementation for set_schema_version method
		use db::postgres::schema::sub_cluster::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let mut sub_cluster_addresses = Vec::new();

		let result = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => sub_cluster
				.select(sub_cluster_address)
				.order(sub_cluster_address.desc())
				.load::<String>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => sub_cluster
				.select(sub_cluster_address)
				.order(sub_cluster_address.desc())
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

					sub_cluster_addresses.push(cluster_add);
				},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
		if sub_cluster_addresses.is_empty() {
			Ok(None)
		} else {
			Ok(Some(sub_cluster_addresses))
		}
	}
}
