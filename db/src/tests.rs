#[cfg(test)]
mod tests {
	mod bigint {
		#[cfg(test)]
		mod tests {
			use crate::utils::big_int::{FromBigInt, ToBigInt};
			use num_bigint::BigInt;

			#[test]
			#[serial_test::serial]
			fn test_u128_to_big_int() {
				let value: u128 = 100_000_000_000_000;
				let big_int = value.get_big_int();
				let expected_big_int = num_bigint::BigInt::from(100_000_000_000_000u128);

				assert_eq!(big_int, expected_big_int);
			}

			#[test]
			#[serial_test::serial]
			fn test_u64_to_big_int() {
				let value: u64 = 100_000_000;
				let big_int = value.get_big_int();
				let expected_big_int = num_bigint::BigInt::from(100_000_000u64);

				assert_eq!(big_int, expected_big_int);
			}

			#[test]
			#[serial_test::serial]
			fn test_u32_to_big_int() {
				let value: u32 = 100_000_000;
				let big_int = value.get_big_int();
				let expected_big_int = num_bigint::BigInt::from(100_000_000u32);

				assert_eq!(big_int, expected_big_int);
			}

			#[test]
			#[serial_test::serial]
			fn test_big_int_to_u128() {
				let big_int = num_bigint::BigInt::from(100_000_000_000_000u128);
				let value = u128::from_big_int(&big_int);
				let expected_value: u128 = 100_000_000_000_000;

				assert_eq!(value, expected_value);
			}

			#[test]
			#[serial_test::serial]
			fn test_big_int_to_u64() {
				let big_int = num_bigint::BigInt::from(100_000_000u64);
				let value = u64::from_big_int(&big_int);
				let expected_value: u64 = 100_000_000;

				assert_eq!(value, expected_value);
			}

			#[test]
			#[serial_test::serial]
			fn test_big_int_to_u32() {
				let big_int = num_bigint::BigInt::from(100_000_000u32);
				let value = u32::from_big_int(&big_int);
				let expected_value: u32 = 100_000_000;

				assert_eq!(value, expected_value);
			}

			#[test]
			#[serial_test::serial]
			/// Tests that the addition of two big integers works.
			fn test_big_uint_addition() {
				let number_a: BigInt = 100_000_000_000_000_000u128.get_big_int();
				let number_b: BigInt = 100_000_000_000_000_000u128.get_big_int();

				let sum = number_a.checked_add(&number_b).unwrap();
				assert_eq!(sum, 200_000_000_000_000_000u128.get_big_int());
			}
		}
	}
	mod db {
		use crate::{
			cassandra::*,
			utils::{FromByteArray, ToByteArray},
		};
		use primitives::arithmetic::ScalarBig;
		use scylla::_macro_internal::CqlValue;

		#[tokio::test]
		#[serial_test::serial]
		async fn test_db_operations() {
			let session = DatabaseManager::initialize(None).await.unwrap();
			if !DatabaseManager::is_session_healthy(session.clone()).await {
				DatabaseManager::replace_session().await.unwrap();
			}
			// Creates keyspace `l1x_test` with `L1XTest` replication strategy and replication
			// factor of 1
			let query_results = match session.query("CREATE KEYSPACE IF NOT EXISTS l1x_test WITH replication = {'class' : 'NetworkTopologyStrategy'} AND durable_writes = true", &[]).await {
                Ok(result) => result,
                Err(_) => return

            };
			assert_eq!(query_results.rows_or_empty().len(), 0);

			// Creates table with a composite primary key
			session
                .query(
                    "CREATE TABLE IF NOT EXISTS l1x_test.test_table (pk int, ck int, value text, primary key (pk, ck))",
                    &[],
                )
                .await.unwrap();

			// Inserts a row into the table
			session
				.query(
					"INSERT INTO l1x_test.test_table (pk, ck, value) VALUES (?, ?, ?)",
					(1, 1, "hello l1x node"),
				)
				.await
				.unwrap();

			// Inserts an empty row into the table
			session
                .query(
                    "INSERT INTO l1x_test.test_table (pk, ck, value) VALUES (1, 2, 'hello again l1x node')",
                    &[],
                )
                .await.unwrap();

			let query_result = session
				.query("SELECT pk, ck, value FROM l1x_test.test_table", &[])
				.await
				.unwrap();

			let rows = query_result.rows_or_empty();

			assert_eq!(rows.len(), 2);
		}

		#[tokio::test]
		#[serial_test::serial]
		async fn test_byte_u128_operations() {
			// Create a session
			let session = DatabaseManager::initialize(None).await.unwrap();
			if !DatabaseManager::is_session_healthy(session.clone()).await {
				DatabaseManager::replace_session().await.unwrap();
			}
			let amount: u128 = 152_000_000_000u128;
			let amount_bytes_u128: ScalarBig = amount.to_byte_array_le(ScalarBig::default());

			// Create the keyspace if it doesn't exist
			session
                .query(
                    "CREATE KEYSPACE IF NOT EXISTS l1x_test WITH replication = {'class' : 'NetworkTopologyStrategy'} AND durable_writes = true",
                    &[],
                )
                .await
                .unwrap();

			// Create the table if it doesn't exist
			session
                .query(
                    "CREATE TABLE IF NOT EXISTS l1x_test.byte_table (pk int, amount blob, primary key (pk))",
                    &[],
                )
                .await
                .unwrap();

			// Insert a row into the table
			session
				.query(
					"INSERT INTO l1x_test.byte_table (pk, amount) VALUES (?, ?)",
					(1, amount_bytes_u128),
				)
				.await
				.unwrap();

			// Retrieve the inserted row
			let query_result = session
				.query("SELECT pk, amount FROM l1x_test.byte_table WHERE pk = ?", (1,))
				.await
				.unwrap();

			let row = query_result.first_row().unwrap();
			let cql_val = row.columns.get(1).unwrap();
			match cql_val {
				None => {},
				Some(val) => match val {
					CqlValue::Blob(x) => {
						let retrieved_amount = u128::from_byte_array(x);
						assert_eq!(retrieved_amount, amount);
					},
					_ => {},
				},
			}
		}

		#[test]
		#[serial_test::serial]
		fn test_u128_to_byte_array_le() {
			let value: u128 = 100_000_000;
			let bytes: [u8; 16] = value.to_byte_array_le(Default::default());
			let expected_bytes: [u8; 16] = [0, 225, 245, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

			assert_eq!(bytes, expected_bytes);
		}

		#[test]
		#[serial_test::serial]
		fn test_u64_to_byte_array_le() {
			let value: u64 = 100_000_000_000;
			let bytes: [u8; 8] = value.to_byte_array_le(Default::default());
			let expected_bytes: [u8; 8] = [0, 232, 118, 72, 23, 0, 0, 0];

			assert_eq!(bytes, expected_bytes);
		}

		#[test]
		#[serial_test::serial]
		fn test_u32_to_byte_array_le() {
			let value: u32 = 100_000_000;
			let bytes: [u8; 4] = value.to_byte_array_le(Default::default());
			let expected_bytes: [u8; 4] = [0, 225, 245, 5];

			assert_eq!(bytes, expected_bytes);
		}

		#[test]
		#[serial_test::serial]
		fn test_u128_from_byte_array() {
			let bytes: [u8; 16] = [210, 2, 150, 73, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
			let value: u128 = u128::from_byte_array(&bytes);

			assert_eq!(value, 142968488658);
		}

		#[test]
		#[serial_test::serial]
		fn test_u64_from_byte_array() {
			let bytes: [u8; 8] = [18, 186, 157, 229, 0, 0, 0, 0];
			let value: u64 = u64::from_byte_array(&bytes);

			assert_eq!(value, 3852319250);
		}
	}
}
