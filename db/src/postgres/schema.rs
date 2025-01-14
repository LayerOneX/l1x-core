// @generated automatically by Diesel CLI.

// sql types for postgres
pub mod sql_types {
	#[derive(diesel::sql_types::SqlType)]
	#[diesel(postgres_type(name = "accounttype"))]
	pub struct Accounttype;
}

// Table definitions for postgres
diesel::table! {
	use diesel::sql_types::*;
	use super::sql_types::Accounttype;

	account (address) {
		address -> Varchar,
		account_type -> Nullable<Accounttype>,
		balance -> Nullable<Numeric>,
		nonce -> Nullable<Numeric>,
	}
}

diesel::table! {
	block_head (cluster_address) {
		cluster_address -> Varchar,
		block_number -> Nullable<Numeric>,
		block_hash -> Nullable<Varchar>,
	}
}

diesel::table! {
	block_header (block_number) {
		block_number -> Numeric,
		block_hash -> Nullable<Varchar>,
		parent_hash -> Nullable<Varchar>,
		timestamp -> Nullable<Timestamptz>,
		block_type -> Nullable<Int2>,
		num_transactions -> Nullable<Int4>,
		cluster_address -> Nullable<Varchar>,
		block_version -> Nullable<Int4>,
		state_hash -> Nullable<Varchar>,
		epoch -> Numeric,
	}
}

diesel::table! {
	block_meta_info (cluster_address, block_number) {
		cluster_address -> Varchar,
		block_number -> Numeric,
		block_executed -> Nullable<Bool>,
	}
}

diesel::table! {
	block_proposer (address, cluster_address, epoch) {
		address -> Varchar,
		cluster_address -> Varchar,
		epoch -> Numeric,
		selected_next -> Nullable<Bool>,
	}
}

diesel::table! {
	block_transaction (transaction_hash, block_number) {
		transaction_hash -> Varchar,
		block_number -> Numeric,
		block_hash -> Nullable<Varchar>,
		fee_used -> Nullable<Numeric>,
		from_address -> Nullable<Varchar>,
		timestamp -> Nullable<Timestamptz>,
		transaction -> Nullable<Bytea>,
		tx_sequence -> Nullable<Numeric>,
	}
}

diesel::table! {
	transaction_metadata (transaction_hash) {
		transaction_hash -> Varchar,
		fee -> Numeric,
		burnt_gas -> Numeric,
		is_successful -> Bool,
	}
}
diesel::table! {
	cluster (cluster_address) {
		cluster_address -> Varchar,
	}
}

diesel::table! {
	contract (address) {
		address -> Varchar,
		access -> Nullable<Int2>,
		code -> Nullable<Varchar>,
		owner_address -> Nullable<Varchar>,
		#[sql_name = "type"]
		type_ -> Nullable<Int2>,
	}
}

diesel::table! {
	contract_instance (instance_address, key) {
		instance_address -> Varchar,
		key -> Varchar,
		value -> Nullable<Varchar>,
	}
}

diesel::table! {
	contract_instance_contract_code_map (instance_address, contract_address) {
		instance_address -> Varchar,
		contract_address -> Varchar,
		owner_address -> Nullable<Varchar>,
	}
}

diesel::table! {
	event (id) {
		id -> BigSerial,
		transaction_hash -> Varchar,
		timestamp -> Timestamptz,
		block_number -> Nullable<Numeric>,
		contract_address -> Nullable<Varchar>,
		event_data -> Nullable<Varchar>,
		event_type -> Nullable<Int2>,
		topic0 -> Nullable<Varchar>,
		topic1 -> Nullable<Varchar>,
		topic2 -> Nullable<Varchar>,
		topic3 -> Nullable<Varchar>,
	}
}

diesel::table! {
	node_info (address) {
		address -> Varchar,
		peer_id -> Varchar,
		cluster_address -> Nullable<Varchar>,
		ip_address -> Nullable<Varchar>,
		joined_epoch -> Nullable<Numeric>,
		metadata -> Nullable<Varchar>,
		signature -> Nullable<Varchar>,
		verifying_key -> Nullable<Varchar>,
	}
}

diesel::table! {
	staking_account (pool_address, account_address) {
		pool_address -> Varchar,
		account_address -> Varchar,
		balance -> Nullable<Numeric>,
	}
}

diesel::table! {
	staking_pool (pool_address) {
		pool_address -> Varchar,
		cluster_address -> Nullable<Varchar>,
		contract_instance_address -> Nullable<Varchar>,
		created_block_number -> Nullable<Numeric>,
		max_pool_balance -> Nullable<Numeric>,
		max_stake -> Nullable<Numeric>,
		min_pool_balance -> Nullable<Numeric>,
		min_stake -> Nullable<Numeric>,
		pool_owner -> Nullable<Varchar>,
		staking_period -> Nullable<Varchar>,
		updated_block_number -> Nullable<Numeric>,
	}
}

diesel::table! {
	sub_cluster (sub_cluster_address) {
		sub_cluster_address -> Varchar,
	}
}

diesel::table! {
	validator (address, epoch) {
		address -> Varchar,
		cluster_address -> Nullable<Varchar>,
		epoch -> Numeric,
		stake -> Nullable<Numeric>,
		xscore -> Numeric,
	}
}

diesel::table! {
	vote (block_hash, validator_address) {
		block_hash -> Varchar,
		block_number -> Nullable<Numeric>,
		cluster_address -> Nullable<Varchar>,
		validator_address -> Varchar,
		signature -> Nullable<Varchar>,
		verifying_key -> Nullable<Varchar>,
		voting -> Nullable<Bool>,
		epoch -> Nullable<Numeric>,
	}
}

diesel::table! {
	vote_result (block_hash, validator_address) {
		block_hash -> Varchar,
		block_number -> Nullable<Numeric>,
		cluster_address -> Nullable<Varchar>,
		validator_address -> Varchar,
		signature -> Nullable<Varchar>,
		verifying_key -> Nullable<Varchar>,
		vote_passed -> Nullable<Bool>,
		epoch -> Nullable<Numeric>,
		votes -> Nullable<Bytea>,
	}
}

diesel::table! {
	genesis_epoch_block (block_number) {
		block_number -> Numeric,
		epoch ->  Numeric,
	}
}

diesel::table! {
    node_health (measured_peer_id, peer_id, epoch) {
        measured_peer_id -> Varchar,
        peer_id -> Varchar,
        epoch -> Numeric,
        joined_epoch -> Numeric,
        uptime_percentage -> Double,
        response_time_ms -> Numeric,
        transaction_count -> Numeric,
        block_proposal_count -> Numeric,
        anomaly_score -> Double,
        node_health_version -> Integer,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
	account,
	block_head,
	block_header,
	block_meta_info,
	block_proposer,
	block_transaction,
	transaction_metadata,
	cluster,
	contract,
	contract_instance,
	contract_instance_contract_code_map,
	event,
	node_info,
	staking_account,
	staking_pool,
	sub_cluster,
	validator,
	vote,
	vote_result,
	node_health,
);
