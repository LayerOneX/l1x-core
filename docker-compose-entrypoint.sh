#!/bin/bash

DOCKER_COMPOSE=false

# Check for both possible command names
if command -v docker-compose &>/dev/null; then
  DOCKER_COMPOSE="docker-compose"
elif command -v docker compose &>/dev/null; then
  DOCKER_COMPOSE="docker compose"
fi

# Check if v2_core_db container is already running
if ! docker ps -f "name=l1x_core_db" --format '{{.Names}}' | grep -q "l1x_core_db"; then
    echo "v2_core_db is not running. Starting it..."
    $DOCKER_COMPOSE -f ./docker-compose.yml up -d l1x_core_db

    echo "Waiting for 30 seconds before starting l1x_core_node"
    sleep 30
else
    echo "v2_core_db is already running."
fi

# Check if v2_core_server container is already running
if docker ps -f "name=l1x_core_node" --format '{{.Names}}' | grep -q "l1x_core_node"; then
    echo "l1x_core_node is already running. Stopping it..."
    $DOCKER_COMPOSE -f ./docker-compose.yml down l1x_core_node
fi

$DOCKER_COMPOSE -f ./docker-compose.yml up -d --build l1x_core_node

echo "All containers are up!"