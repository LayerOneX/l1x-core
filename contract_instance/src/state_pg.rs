use anyhow::Error;
use async_trait::async_trait;
use db::postgres::{
	pg_models::{
		NewContractInstance, NewContractInstanceContractCodeMap, QueryContractInstance,
		QueryContractInstanceContractCodeMap,
	},
	postgres::{PgConnectionType, PostgresDBConn},
};
use db_traits::{base::BaseState, contract_instance::ContractInstanceState};
use diesel::{self, prelude::*, upsert::excluded, ExpressionMethods};
use primitives::{Address, ContractInstanceKey, ContractInstanceValue};
use system::contract_instance::ContractInstance;
pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<ContractInstance> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, _contract_instance: &ContractInstance) -> Result<(), Error> {
		use db::postgres::schema::contract_instance_contract_code_map::dsl::*;
		let contract_addr = hex::encode(_contract_instance.contract_address);
		let instance_addr = hex::encode(_contract_instance.instance_address);
		let owner_addr = hex::encode(_contract_instance.owner_address);
		let new_contract_instance = NewContractInstanceContractCodeMap {
			instance_address: instance_addr,
			contract_address: contract_addr,
			owner_address: Some(owner_addr),
		};

		if self.pg.config.dev_mode {
			match &self.pg.conn {
				PgConnectionType::TxConn(conn) =>
					diesel::insert_into(contract_instance_contract_code_map)
						.values(new_contract_instance)
						.on_conflict((instance_address, contract_address))
						.do_update()
						.set(owner_address.eq(excluded(owner_address)))
						.execute(*conn.lock().await),
				PgConnectionType::PgConn(conn) =>
					diesel::insert_into(contract_instance_contract_code_map)
						.values(new_contract_instance)
						.on_conflict((instance_address, contract_address))
						.do_update()
						.set(owner_address.eq(excluded(owner_address)))
						.execute(&mut *conn.lock().await),
			}?;
		} else {
			match &self.pg.conn {
				PgConnectionType::TxConn(conn) =>
					diesel::insert_into(contract_instance_contract_code_map)
						.values(new_contract_instance)
						.execute(*conn.lock().await),
				PgConnectionType::PgConn(conn) =>
					diesel::insert_into(contract_instance_contract_code_map)
						.values(new_contract_instance)
						.execute(&mut *conn.lock().await),
			}?;
		}

		Ok(())
	}

	async fn update(&self, _contract_instance: &ContractInstance) -> Result<(), Error> {
		// Implementation for update method
		todo!()
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
		todo!()
	}
}
#[async_trait]
impl<'a> ContractInstanceState for StatePg<'a> {
	async fn get_contract_instance(
		&self,
		instance_addr: &Address,
	) -> Result<ContractInstance, Error> {
		use db::postgres::schema::contract_instance_contract_code_map::dsl::*;

		let res: Result<Vec<QueryContractInstanceContractCodeMap>, diesel::result::Error> =
			match &self.pg.conn {
				PgConnectionType::TxConn(conn) => contract_instance_contract_code_map
					.filter(instance_address.eq(hex::encode(instance_addr)))
					.load(*conn.lock().await),
				PgConnectionType::PgConn(conn) => contract_instance_contract_code_map
					.filter(instance_address.eq(hex::encode(instance_addr)))
					.load(&mut *conn.lock().await),
			};

		match res {
			Ok(results) => {
				if let Some(query_results) = results.first() {
					println!("query_results: {:?}", query_results);
					let mut instance_addr: Address = [0; 20];
					let mut owner_addr: Address = [0; 20];
					let mut contract_addr: Address = [0; 20];

					let i_addr = hex::decode(query_results.instance_address.clone())?;

					if i_addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&i_addr);
						instance_addr = array;
					}

					let ow_addr = hex::decode(
						query_results.owner_address.clone().unwrap_or_else(|| "".to_string()),
					)?;

					if ow_addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&ow_addr);
						owner_addr = array;
					}

					let con_addr = hex::decode(query_results.contract_address.clone())?;

					if con_addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&con_addr);
						contract_addr = array;
					}

					let query_contract_code_map = ContractInstance {
						instance_address: instance_addr,
						contract_address: contract_addr,
						owner_address: owner_addr,
					};
					return Ok(query_contract_code_map);
				} else {
					return Err(anyhow::anyhow!("No matching records found for contract address"));
				};
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn store_state_key_value(
		&self,
		instance_addr: &Address,
		key_addr: &ContractInstanceKey,
		obj_value: &ContractInstanceValue,
	) -> Result<(), Error> {
		use db::postgres::schema::contract_instance::dsl::*;
		let update = {
			// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
			// 	&mut *self.pg.conn.pool_conn.lock().await;

			let new_contract_instance = NewContractInstance {
				instance_address: hex::encode(instance_addr),
				key: Some(hex::encode(key_addr)),
				value: Some(hex::encode(obj_value)),
			};

			match &self.pg.conn {
				PgConnectionType::TxConn(conn) => {
					match diesel::insert_into(contract_instance)
						.values(new_contract_instance)
						.on_conflict((instance_address, key))
						.do_update()
						.set(value.eq(excluded(value)))
						.execute(*conn.lock().await)
					{
						Ok(_) => false,
						Err(_e) => true,
					}
				},
				PgConnectionType::PgConn(conn) => {
					match diesel::insert_into(contract_instance)
						.values(new_contract_instance)
						.on_conflict((instance_address, key))
						.do_update()
						.set(value.eq(excluded(value)))
						.execute(&mut *conn.lock().await)
					{
						Ok(_) => false,
						Err(_e) => true,
					}
				},
			}
		};
		if update {
			self.update_state_key_value(instance_addr, key_addr, obj_value).await
		} else {
			Ok(())
		}
	}

	async fn update_state_key_value(
		&self,
		_instance_address: &Address,
		_key: &ContractInstanceKey,
		_value: &ContractInstanceValue,
	) -> Result<(), Error> {
		use db::postgres::schema::contract_instance::dsl::*;
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => {
				diesel::update(
					contract_instance.filter(instance_address.eq(hex::encode(_instance_address))),
				)
				.set((key.eq(hex::encode(_key)), value.eq(hex::encode(_value)))) // set new values for balance and nonce
				.execute(*conn.lock().await)
			},
			PgConnectionType::PgConn(conn) => {
				diesel::update(
					contract_instance.filter(instance_address.eq(hex::encode(_instance_address))),
				)
				.set((key.eq(hex::encode(_key)), value.eq(hex::encode(_value)))) // set new values for balance and nonce
				.execute(&mut *conn.lock().await)
			},
		}?;
		Ok(())
	}

	async fn delete_state_key_value(
		&self,
		_instance_address: &Address,
		_key: &ContractInstanceKey,
	) -> Result<(), Error> {
		use db::postgres::schema::contract_instance::dsl::*;
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::delete(
				contract_instance.filter(
					instance_address
						.eq(hex::encode(_instance_address))
						.and(key.eq(hex::encode(_key))),
				),
			)
			.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::delete(
				contract_instance.filter(
					instance_address
						.eq(hex::encode(_instance_address))
						.and(key.eq(hex::encode(_key))),
				),
			)
			.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn get_state_key_value(
		&self,
		instance_addr: &Address,
		key_addr: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		use db::postgres::schema::contract_instance::dsl::*;
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let instance_addr = hex::encode(instance_addr);
		let key_addr = hex::encode(key_addr);

		let res: Result<QueryContractInstance, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => contract_instance
				.filter(instance_address.eq(&instance_addr).and(key.eq(key_addr)))
				.first(*conn.lock().await),
			PgConnectionType::PgConn(conn) => contract_instance
				.filter(instance_address.eq(&instance_addr).and(key.eq(key_addr)))
				.first(&mut *conn.lock().await),
		};

		match res {
			Ok(query_results) => {
				let contract_value: ContractInstanceValue =
					hex::decode(query_results.value.unwrap().clone())?;
				Ok(Some(contract_value))
			},
			Err(_e) => Ok(None), //return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn is_valid_contract_instance(&self, instance_addr: &Address) -> Result<bool, Error> {
		use db::postgres::schema::contract_instance_contract_code_map::dsl::*;
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let instace_addr = hex::encode(instance_addr);
		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => contract_instance_contract_code_map
				.filter(instance_address.eq(instace_addr))
				.load::<QueryContractInstance>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => contract_instance_contract_code_map
				.filter(instance_address.eq(instace_addr))
				.load::<QueryContractInstance>(&mut *conn.lock().await),
		}?;

		Ok(!res.is_empty())
	}
}
