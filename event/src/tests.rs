#[cfg(test)]
mod tests {
	use crate::event_state::{*};
	use anyhow::{anyhow, Error};
	use db::db::{Database, DbTxConn};
	use ethereum_types::{H160, H256};
	use primitives::*;
	use system::{config::Config, event::Event};
	use types::eth::{filter::Filter};
	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn perform_table_cleanup() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let event_state = EventState::new(&db_pool_conn).await.unwrap();
		truncate_table(&event_state).await;
	}

	pub async fn truncate_table<'a>(event_state: &EventState<'a>) -> Result<(), Error> {
		match event_state.raw_query("TRUNCATE event;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Truncate table failed")),
		};
		Ok(())
	}
	async fn get_test_event() -> Event {
		let transaction_hash: TransactionHash = [0u8; 32];
		let event_data: EventData = vec![1, 2, 3, 4, 5];
		let block_number = 3;
		let contract_address = [1u8; 20];
		let topics: Option<Vec<H256>> =
			Some(vec![H256::from([1u8; 32]), H256::from([2u8; 32]), H256::from([3u8; 32])]);

		Event {
			transaction_hash,
			event_data,
			block_number,
			event_type: 1,
			contract_address,
			topics,
		}
	}

	async fn get_test_events() -> (Event, Event) {
		let transaction_hash: TransactionHash = [0u8; 32];
		let event_data: EventData = vec![1, 2, 3, 4, 5];
		let block_number = 3;
		let contract_address = [1u8; 20];
		let topics: Option<Vec<H256>> =
			Some(vec![H256::from([1u8; 32]), H256::from([2u8; 32]), H256::from([3u8; 32])]);

		let e1 = Event {
			transaction_hash,
			event_data,
			block_number,
			event_type: system::event::EventType::EVM as i8,
			contract_address,
			topics,
		};

		let mut event_signature: [u8; 32] = [0u8; 32];
		event_signature.copy_from_slice(
			hex::decode("38451f482d8242a7047647c1290d44d36ab4fd8450b02c3d2777ee0acf1354a3")
				.unwrap()
				.as_ref(),
		);
		let mut topic1: [u8; 32] = [0u8; 32];
		topic1.copy_from_slice(
			hex::decode("00000000000000000000000075104938baa47c54a86004ef998cc76c2e616289")
				.unwrap()
				.as_ref(),
		);
		let e2 = Event {
			transaction_hash: [1u8; 32],
			event_data: vec![43, 144, 213],
			block_number: 45,
			event_type: system::event::EventType::EVM as i8,
			contract_address: [5u8; 20],
			topics: Some(vec![
				H256::from(event_signature),
				H256::from(topic1),
				H256::from([9u8; 32]),
			]),
		};

		(e1, e2)
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_create_event() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let event_state = EventState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let event = get_test_event().await;

		// Act
		let result = event_state.create_event(&event).await;

		// Assert
		assert!(result.is_ok());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_get_event() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let event_state = EventState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let event = get_test_event().await;
		event_state.create_event(&event).await.unwrap();

		// Act
		let retrieved_event = event_state.get_events(&event.transaction_hash, 0).await.unwrap();

		// Assert
		assert_eq!(retrieved_event, vec![event.event_data]);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_is_valid_event() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let event_state = EventState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let event = get_test_event().await;
		event_state.create_event(&event).await.unwrap();

		// Act
		let is_valid = event_state.is_valid_event(&event.transaction_hash).await.unwrap();

		// Assert
		assert!(is_valid);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_get_filtered_event() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let event_state = EventState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;

		// Populate the event table with some example events
		let (e1, e2) = get_test_events().await;

		event_state.create_event(&e1).await.unwrap();
		event_state.create_event(&e2).await.unwrap();

		let filter = Filter::new()
			.address("0505050505050505050505050505050505050505".parse::<H160>().unwrap())
			.event("Event1(address,string,address,uint256)")
			.topic1(H256::from(
				"0x75104938baa47c54a86004ef998cc76c2e616289".parse::<H160>().unwrap(),
			))
			.topic2(
				"0x0909090909090909090909090909090909090909090909090909090909090909"
					.parse::<H256>()
					.unwrap(),
			)
			.from_block(types::eth::block::BlockNumber::Num(5))
			.to_block(types::eth::block::BlockNumber::Num(50));

		let result = event_state.get_filtered_events(filter).await;
		println!("result: {:?}", result);
		let result = result.unwrap();
		let event = &result[0];
		assert_eq!(
			event.address,
			"0505050505050505050505050505050505050505".parse::<H160>().unwrap()
		);
		assert_eq!(event.block_number.unwrap().as_u64(), 45);
		assert_eq!(event.data, vec![43, 144, 213].into());
	}
}
