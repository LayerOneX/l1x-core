use directories::UserDirs;
use std::{fs::create_dir_all, path::Path};
//use system::config::Config as SystemConfig;

#[derive(Debug)]
pub struct DatabaseManager;

impl DatabaseManager {
	pub(crate) fn new(rocksdb_name: String) -> String {
		let user_dirs = UserDirs::new().expect("Couldn't fetch home directory");
		let home_dir = user_dirs.home_dir().to_path_buf();
		//println!("Working directory: {:?}", home_dir);

		// Create 'l1x' directory inside the working directory
		let l1x_dir = home_dir.join(rocksdb_name);

		// Check if directory already exists
		if Path::new(&l1x_dir).exists() {
			//println!("Directory already exists");
		} else {
			create_dir_all(&l1x_dir).expect("Couldn't create l1x directory");
			//println!("Created l1x directory");
		}

		// Convert l1x_dir to a String and return it
		l1x_dir.to_string_lossy().to_string()
	}
}
