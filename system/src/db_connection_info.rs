/// Specifies node database connection info
pub struct DbConnectionInfo {
	pub host: String,
	pub username: String,
	pub password: String,
	pub db_name: String,
	pub pool_size: u32,
}
