use anyhow::Error;
use async_trait::async_trait;
use db::postgres::{
	pg_models::{NewContract, QueryContract},
	postgres::{PgConnectionType, PostgresDBConn},
};
use db_traits::{base::BaseState, contract::ContractState};
use diesel::{self, prelude::*};
use primitives::{AccessType, Address};
use system::contract::Contract;

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}
#[async_trait]
impl<'a> BaseState<Contract> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, _contract: &Contract) -> Result<(), Error> {
		use db::postgres::schema::contract::dsl::*;

		let new_contract = NewContract {
			address: hex::encode(_contract.address),
			access: Some(_contract.access.into()),
			code: Some(hex::encode(_contract.code.clone())),
			owner_address: Some(hex::encode(_contract.owner_address)),
			type_: Some(_contract.r#type.into()),
		};

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) =>
				diesel::insert_into(contract).values(new_contract).execute(*conn.lock().await)?,
			PgConnectionType::PgConn(conn) => diesel::insert_into(contract)
				.values(new_contract)
				.execute(&mut *conn.lock().await)?,
		};
		Ok(())
	}

	async fn update(&self, _contract: &Contract) -> Result<(), Error> {
		use db::postgres::schema::contract::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let addr = hex::encode(_contract.address);
		let contract_code = hex::encode(_contract.code.clone());

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => {
				diesel::update(contract.filter(address.eq(addr)))
					.set(code.eq(contract_code)) // set new values for balance and nonce
					.execute(*conn.lock().await)
			},
			PgConnectionType::PgConn(conn) => {
				diesel::update(contract.filter(address.eq(addr)))
					.set(code.eq(contract_code)) // set new values for balance and nonce
					.execute(&mut *conn.lock().await)
			},
		}?;
		Ok(())
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

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}
#[async_trait]
impl<'a> ContractState for StatePg<'a> {
	async fn get_all_contract(&self) -> Result<Vec<Contract>, Error> {
		use db::postgres::schema::contract::dsl::*;
		let mut contracts_list = vec![];
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		let res: Result<Vec<QueryContract>, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => contract.load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => contract.load(&mut *conn.lock().await),
		};

		match res {
			Ok(result) => {
				for query_contract in result {
					let mut contract_add: Address = [0; 20];
					let addr = hex::decode(query_contract.address.clone())?;
					if addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&addr);
						contract_add = array;
					}

					let mut owner_add: Address = [0; 20];
					let owner = hex::decode(query_contract.owner_address.clone().unwrap())?;
					if owner.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&owner);
						owner_add = array;
					}
					let contract_code = hex::decode(query_contract.code.clone().unwrap())?;

					let con = Contract {
						address: contract_add,
						access: query_contract.access.unwrap() as i8,
						r#type: query_contract.type_.unwrap() as i8,
						code: contract_code,
						owner_address: owner_add,
					};

					contracts_list.push(con);
				}
				Ok(contracts_list)
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn get_contract(&self, _address: &Address) -> Result<Contract, Error> {
		use db::postgres::schema::contract::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let encoded_address = hex::encode(_address);
		let res: Result<Vec<QueryContract>, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) =>
				contract.filter(address.eq(encoded_address.clone())).load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => contract
				.filter(address.eq(encoded_address.clone()))
				.load(&mut *conn.lock().await),
		};

		match res {
			Ok(result) => {
				if let Some(query_contract) = result.first() {
					let mut contract_add: Address = [0; 20];
					let addr = hex::decode(query_contract.address.clone())?;
					if addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&addr);
						contract_add = array;
					}

					let mut owner_add: Address = [0; 20];
					let owner = hex::decode(query_contract.owner_address.clone().unwrap())?;
					if owner.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&owner);
						owner_add = array;
					}
					let contract_code = hex::decode(query_contract.code.clone().unwrap())?;

					let con = Contract {
						address: contract_add,
						access: query_contract.access.unwrap() as i8,
						r#type: query_contract.type_.unwrap() as i8,
						code: contract_code,
						owner_address: owner_add,
					};

					return Ok(con);
				} else {
					return Err(anyhow::anyhow!("No matching records found for contract address"));
				};
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn get_contract_owner(&self, _address: &Address) -> Result<(AccessType, Address), Error> {
		use db::postgres::schema::contract::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let encoded_address = hex::encode(_address);

		let res: Result<QueryContract, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) =>
				contract.filter(address.eq(&encoded_address)).first(*conn.lock().await),
			PgConnectionType::PgConn(conn) =>
				contract.filter(address.eq(&encoded_address)).first(&mut *conn.lock().await),
		};

		match res {
			Ok(result) => {
				let mut owner_add: Address = [0; 20];
				let owner = hex::decode(result.owner_address.clone().unwrap())?;
				if owner.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&owner);
					owner_add = array;
				}

				Ok((result.access.unwrap() as i8, owner_add))
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn is_valid_contract(&self, _address: &Address) -> Result<bool, Error> {
		use db::postgres::schema::contract::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let encoded_address = hex::encode(_address);
		let count: i64 = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => contract
				.filter(address.eq(encoded_address))
				.count()
				.get_result(*conn.lock().await),
			PgConnectionType::PgConn(conn) => contract
				.filter(address.eq(encoded_address))
				.count()
				.get_result(&mut *conn.lock().await),
		}?;

		Ok(count > 0)
	}
}
