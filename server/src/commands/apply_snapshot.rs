use std::{
    fs::read_to_string,
    path::PathBuf,
};

use directories::UserDirs;
use structopt::StructOpt;
use toml;

use db::db::Database;
use node_crate::sync::apply_db_dump;
use system::config::Config;

#[derive(Debug, StructOpt)]
#[structopt(name = "apply_snapshot")]
pub struct ApplySnapshotCmd {
    #[structopt(long = "path", short = "w")]
    working_dir: Option<PathBuf>,
}

impl ApplySnapshotCmd {
    pub async fn execute(&self) {
        // Determine the working directory
        pretty_env_logger::init();

        let working_dir = match &self.working_dir {
            Some(dir) => dir.clone(),
            None => {
                let user_dirs = UserDirs::new().expect("Couldn't fetch home directory");
                user_dirs.home_dir().to_path_buf()
            }
        };
        println!("Working directory: {:?}", working_dir);

        if !working_dir.exists() {
            println!("l1x folder does not exist");
            return;
        }

        // Construct path to config.toml and genesis.json
        let mut config_path = working_dir.clone();
        config_path.push("config.toml");


        // Read and parse config.toml
        let mut parsed_config: Config = Config::default();
        match read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(config) => {
                    parsed_config = config;
                }
                Err(e) => println!("Could not parse config.toml: {:?}", e),
            },
            Err(e) => println!("Could not read config.toml: {:?}", e),
        }
        Database::new(&parsed_config).await;
        // Apply snapshot
        match apply_db_dump(parsed_config).await {
            Ok(_) => {
                log::info!("✅ Snapshot syncing successful ✅");
            }
            Err(e) => {
                panic!("Unable to sync snapshot: {:?}", e);
            }
        }
    }
}