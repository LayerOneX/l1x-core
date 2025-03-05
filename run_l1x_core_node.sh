#!/bin/bash

# Navigate to the application directory
cd /l1x/

# Create the data directory structure for L1X if it doesn't exist
if [ ! -d "l1x_data/l1x" ]; then
    mkdir -p l1x_data/l1x

    chmod +x ./l1x_core_node
    # Initialize the l1x_core_node, specifying the working directory
    RUST_LOG=info ./l1x_core_node init -w "l1x_data/l1x"
    # Pause for 30 seconds (likely for initialization tasks to complete)
    sleep 30

    RUST_LOG=info ./l1x_core_node apply_snapshot -w "l1x_data/l1x"
fi

# Check if the data directory exists
if [ -d "l1x_data/l1x" ]; then
    # Announce server startup
    echo "Starting L1X Core Node"
    RUST_LOG=info ./l1x_core_node update-node-info
    
    # Start the server with RUST_LOG set to "info" for logging
    RUST_LOG=info ./l1x_core_node start -w "l1x_data/l1x"
else
    echo "Error: Unable to create data directory structure."
fi