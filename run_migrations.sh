#!/bin/bash

# Read host from config.toml
postgres_host=$(awk -F '"' '/postgres_host/ {print $2; exit}' config.toml)

# Read username from config.toml
username=$(awk -F '"' '/postgres_username/ {print $2; exit}' config.toml)

# Read password from config.toml
password=$(awk -F '"' '/postgres_password/ {print $2; exit}' config.toml)

# Read db_name from config.toml
db_name=$(awk -F '"' '/postgres_db_name/ {print $2; exit}' config.toml)

# Navigate to the migrations directory
cd db/src/postgres

# Run Diesel migrations
diesel migration run --database-url=postgres://$username:$password@$postgres_host/$db_name
