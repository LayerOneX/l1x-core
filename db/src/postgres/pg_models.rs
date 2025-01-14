use crate::postgres::schema::{sql_types::Accounttype, *};
use bigdecimal::BigDecimal;
use chrono::{format::Numeric, NaiveDateTime};
use diesel::{
	self,
	deserialize::{self, FromSql, FromSqlRow, Queryable, QueryableByName},
	expression::AsExpression,
	pg::{Pg, PgValue},
	prelude::{Insertable, *},
	serialize::{self, Output, ToSql},
};
use std::io::Write;
use serde::{Deserialize, Serialize};

#[derive(Eq, PartialEq, Debug, Insertable, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = account)]
pub struct NewAccount {
	pub address: String,
	pub account_type: Option<AccountType>,
	pub balance: Option<BigDecimal>,
	pub nonce: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Selectable)]
#[diesel(table_name = account)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryAccount {
	pub address: String,
	pub account_type: Option<AccountType>,
	pub balance: Option<BigDecimal>,
	pub nonce: Option<BigDecimal>,
}

#[derive(Debug, AsExpression, FromSqlRow, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[diesel(sql_type=Accounttype)]
pub enum AccountType {
	System,
	User,
}

impl FromSql<Accounttype, Pg> for AccountType {
	fn from_sql(bytes: PgValue) -> deserialize::Result<Self> {
		match bytes.as_bytes() {
			b"System" => Ok(AccountType::System),
			b"User" => Ok(AccountType::User),
			_ => Err("Unrecognized enum variant".into()),
		}
	}
}

impl From<AccountType> for system::account::AccountType {
	fn from(account_type: AccountType) -> Self {
		match account_type {
			AccountType::System => system::account::AccountType::System,
			AccountType::User => system::account::AccountType::User,
		}
	}
}

impl ToSql<Accounttype, Pg> for AccountType {
	fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
		match *self {
			AccountType::System => out.write_all(b"System")?,
			AccountType::User => out.write_all(b"User")?,
		}
		Ok(diesel::serialize::IsNull::No)
	}
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = block_header)]
pub struct NewBlockHeader {
	pub block_number: BigDecimal,
	pub block_hash: Option<String>,
	pub parent_hash: Option<String>,
	pub timestamp: Option<NaiveDateTime>,
	pub block_type: Option<i16>,
	pub num_transactions: Option<i32>,
	pub block_version: Option<i32>,
	pub state_hash: Option<String>,
	pub cluster_address: String,
	pub epoch: BigDecimal,
}

#[derive(Eq, PartialEq, Debug, Queryable, Selectable, Serialize, Deserialize, Insertable)]
#[diesel(table_name = block_header)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryBlockHeader {
	pub block_number: BigDecimal,
	pub block_hash: Option<String>,
	pub parent_hash: Option<String>,
	pub timestamp: Option<NaiveDateTime>,
	pub block_type: Option<i16>,
	pub num_transactions: Option<i32>,
	pub cluster_address: Option<String>,
	pub block_version: Option<i32>,
	pub state_hash: Option<String>,
	pub epoch: BigDecimal,
}

#[derive(Eq, PartialEq, Debug, Insertable, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = cluster)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Cluster {
	pub cluster_address: String,
}

// #[derive(Eq, PartialEq, Debug, Queryable, Selectable)]
// #[diesel(table_name = cluster)]
// #[diesel(check_for_backend(diesel::pg::Pg))]
// pub struct QueryCluster {
// 	pub cluster_address: String,
// }

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = block_head)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewBlockHead {
	pub cluster_address: String,
	pub block_number: Option<BigDecimal>,
	pub block_hash: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Serialize, Deserialize, Insertable)]
#[diesel(table_name = block_head)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryBlockHead {
	pub cluster_address: String,
	pub block_number: Option<BigDecimal>,
	pub block_hash: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Insertable, Clone)]
#[diesel(table_name = block_meta_info)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewBlockMetaInfo {
	pub cluster_address: String,
	pub block_number: Option<BigDecimal>,
	pub block_executed: Option<bool>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = block_meta_info)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryBlockMetaInfo {
	pub cluster_address: String,
	pub block_number: BigDecimal,
	pub block_executed: Option<bool>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = block_transaction)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewBlockTransactions {
	pub transaction_hash: String,
	pub block_number: Option<BigDecimal>,
	pub block_hash: Option<String>,
	pub fee_used: Option<BigDecimal>,
	pub from_address: Option<String>,
	pub timestamp: Option<NaiveDateTime>,
	pub transaction: Option<Vec<u8>>,
	pub tx_sequence: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = block_transaction)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryBlockTransactions {
	pub transaction_hash: String,
	pub block_number: BigDecimal,
	pub block_hash: Option<String>,
	pub fee_used: Option<BigDecimal>,
	pub from_address: Option<String>,
	pub timestamp: Option<NaiveDateTime>,
	pub transaction: Option<Vec<u8>>,
	pub tx_sequence: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = transaction_metadata)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewTransactionMetadata {
	pub transaction_hash: String,
	pub fee: BigDecimal,
	pub burnt_gas: BigDecimal,
	pub is_successful: bool,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = transaction_metadata)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryTransactionMetadata {
	pub transaction_hash: String,
	pub fee: BigDecimal,
	pub burnt_gas: BigDecimal,
	pub is_successful: bool,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = contract)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewContract {
	pub address: String,
	pub access: Option<i16>,
	pub code: Option<String>,
	pub owner_address: Option<String>,
	pub type_: Option<i16>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = contract)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryContract {
	pub address: String,
	pub access: Option<i16>,
	pub code: Option<String>,
	pub owner_address: Option<String>,
	pub type_: Option<i16>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = contract_instance)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewContractInstance {
	pub instance_address: String,
	pub key: Option<String>,
	pub value: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = contract_instance)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryContractInstance {
	pub instance_address: String,
	pub key: String,
	pub value: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = contract_instance_contract_code_map)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewContractInstanceContractCodeMap {
	pub instance_address: String,
	pub contract_address: String,
	pub owner_address: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = contract_instance_contract_code_map)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryContractInstanceContractCodeMap {
	pub instance_address: String,
	pub contract_address: String,
	pub owner_address: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = event)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewEvent {
	pub transaction_hash: String,
	pub timestamp: Option<NaiveDateTime>,
	pub block_number: Option<BigDecimal>,
	pub contract_address: Option<String>,
	pub event_data: Option<String>,
	pub event_type: Option<i16>,
	pub topic0: Option<String>,
	pub topic1: Option<String>,
	pub topic2: Option<String>,
	pub topic3: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = event)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryEvent {
	pub id: i64,
	pub transaction_hash: String,
	pub timestamp: NaiveDateTime,
	pub block_number: Option<BigDecimal>,
	pub contract_address: Option<String>,
	pub event_data: Option<String>,
	pub event_type: Option<i16>,
	pub topic0: Option<String>,
	pub topic1: Option<String>,
	pub topic2: Option<String>,
	pub topic3: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = node_info)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewNodeInfo {
	pub address: String,
	pub peer_id: String,
	pub cluster_address: Option<String>,
	pub ip_address: Option<String>,
	pub joined_epoch: Option<BigDecimal>,
	pub metadata: Option<String>,
	pub signature: Option<String>,
	pub verifying_key: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Selectable)]
#[diesel(table_name = node_info)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryNodeInfo {
	pub address: String,
	pub peer_id: String,
	pub cluster_address: Option<String>,
	pub ip_address: Option<String>,
	pub joined_epoch: Option<BigDecimal>,
	pub metadata: Option<String>,
	pub signature: Option<String>,
	pub verifying_key: Option<String>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = genesis_epoch_block)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewGenesisBlock {
	pub block_number: BigDecimal,
	pub epoch: BigDecimal,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = genesis_epoch_block)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryGenesisBlock {
	pub block_number: BigDecimal,
	pub epoch: BigDecimal,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = staking_account)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewStakingAccount {
	pub pool_address: String,
	pub account_address: Option<String>,
	pub balance: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = staking_account)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryStakingAccount {
	pub pool_address: String,
	pub account_address: String,
	pub balance: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = staking_pool)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewStakingPool {
	pub pool_address: String,
	pub cluster_address: Option<String>,
	pub contract_instance_address: Option<String>,
	pub created_block_number: Option<BigDecimal>,
	pub max_pool_balance: Option<BigDecimal>,
	pub max_stake: Option<BigDecimal>,
	pub min_pool_balance: Option<BigDecimal>,
	pub min_stake: Option<BigDecimal>,
	pub pool_owner: Option<String>,
	pub staking_period: Option<String>,
	pub updated_block_number: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = staking_pool)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryStakingPool {
	pub pool_address: String,
	pub cluster_address: Option<String>,
	pub contract_instance_address: Option<String>,
	pub created_block_number: Option<BigDecimal>,
	pub max_pool_balance: Option<BigDecimal>,
	pub max_stake: Option<BigDecimal>,
	pub min_pool_balance: Option<BigDecimal>,
	pub min_stake: Option<BigDecimal>,
	pub pool_owner: Option<String>,
	pub staking_period: Option<String>,
	pub updated_block_number: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = vote)]
pub struct NewVote {
	pub block_hash: String,
	pub block_number: Option<BigDecimal>,
	pub cluster_address: Option<String>,
	pub validator_address: Option<String>,
	pub signature: Option<String>,
	pub verifying_key: Option<String>,
	pub voting: Option<bool>,
	pub epoch: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = vote)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryVote {
	pub block_hash: String,
	pub block_number: Option<BigDecimal>,
	pub cluster_address: Option<String>,
	pub validator_address: String,
	pub signature: Option<String>,
	pub verifying_key: Option<String>,
	pub voting: Option<bool>,
	pub epoch: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = vote_result)]
pub struct NewVoteResult {
	pub block_hash: String,
	pub block_number: Option<BigDecimal>,
	pub cluster_address: Option<String>,
	pub validator_address: Option<String>,
	pub signature: Option<String>,
	pub verifying_key: Option<String>,
	pub vote_passed: Option<bool>,
	pub epoch: Option<BigDecimal>,
	pub votes: Option<Vec<u8>>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = vote_result)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryVoteResult {
	pub block_hash: String,
	pub block_number: Option<BigDecimal>,
	pub cluster_address: Option<String>,
	pub validator_address: String,
	pub signature: Option<String>,
	pub verifying_key: Option<String>,
	pub vote_passed: Option<bool>,
	pub epoch: Option<BigDecimal>,
	pub votes: Option<Vec<u8>>,
}

#[derive(Eq, PartialEq, Debug, Insertable, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = sub_cluster)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct SubCluster {
	pub sub_cluster_address: String,
}

#[derive(Eq, PartialEq, Debug, Insertable)]
#[diesel(table_name = validator)]
pub struct NewValidator {
	pub address: String,
	pub epoch: BigDecimal,
	pub cluster_address: Option<String>,
	pub stake: Option<BigDecimal>,
	pub xscore: Option<BigDecimal>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = validator)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryValidator {
	pub address: String,
	pub cluster_address: Option<String>,
	pub epoch: BigDecimal,
	pub stake: Option<BigDecimal>,
	pub xscore: BigDecimal,
}

#[derive(Eq, PartialEq, Debug, Clone, Insertable)]
#[diesel(table_name = block_proposer)]
pub struct NewBlockProposer {
	pub address: String,
	pub cluster_address: String,
	pub epoch: BigDecimal,
	pub selected_next: Option<bool>,
}

#[derive(Eq, PartialEq, Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = block_proposer)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryBlockProposer {
	pub address: String,
	pub cluster_address: String,
	pub epoch: BigDecimal,
	pub selected_next: Option<bool>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = node_health)]
pub struct NewNodeHealth {
    pub measured_peer_id: String,
    pub peer_id: String,
    pub epoch: BigDecimal,
    pub joined_epoch: BigDecimal,
    pub uptime_percentage: f64,
    pub response_time_ms: BigDecimal,
    pub transaction_count: BigDecimal,
    pub block_proposal_count: BigDecimal,
    pub anomaly_score: f64,
    pub node_health_version: i32,
}

#[derive(Debug, Queryable, Insertable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = node_health)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct QueryNodeHealth {
    pub measured_peer_id: String,
    pub peer_id: String,
    pub epoch: BigDecimal,
    pub joined_epoch: BigDecimal,
    pub uptime_percentage: f64,
    pub response_time_ms: BigDecimal,
    pub transaction_count: BigDecimal,
    pub block_proposal_count: BigDecimal,
    pub anomaly_score: f64,
    pub node_health_version: i32,
}

#[derive(QueryableByName, Debug)]
pub struct MaxBlockNumber {
	#[sql_type = "Numeric"]
	pub max_block_number: Option<BigDecimal>,
}
