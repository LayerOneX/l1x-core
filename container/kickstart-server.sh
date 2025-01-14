#!/bin/bash

set -e

if [ $# -ne 3 ]; then
  echo "Usage: ./kickstart-server.sh <CHAIN_DATA_DIR> <START_DELAY> <BASE_LATENCY"
  exit 1
fi

CHAIN_DATA="${1}"
START_DELAY="${2}"
BASE_LATENCY="${3}"

function setup_latency() {
  let base_latency="${1}"
  # The node container has two networks:
  # 1. DB network
  # 2. L1X network which used used to connect to other nodes
  #
  # Need to setup latency to the interface which is used for L1X network only.
  # Because all nodes are in same network, it's enough to lookup l1x-node1 ip address,
  # calculate the mask and find the interace by it. Assumption: 16bits network mask is used

  # Sleep is required to be sure all nodes already started.
  # Otherwise domain names are not resolved for them
  sleep 1

  # Output of `host l1x-node1`:
  # l1x-node1 has address 172.20.0.5
  local some_node_address="$(host l1x-node1 | awk '{print $4}')"
  # If this script is executed in l1x-node1 context, need to use l1x-node2
  if ip --brief address show | grep "${some_node_address}"; then
    local some_node_address="$(host l1x-node2 | awk '{print $4}')"
    echo ${some_node_address}
  fi
  # Get first two numbers from the ip address: 172.20.0.5 => 172.20.
  local some_node_mask="$(echo ${some_node_address} | awk -F'.' '{print $1 "." $2 "."}')"
  
  echo "Interfaces:"
  ip --brief address show

  # Example of `ip --brief address show` output:
  # lo               UNKNOWN        127.0.0.1/8 ::1/128 
  # eth0@if2225      UP             172.20.0.2/16 
  # eth1@if2227      UP             172.18.0.2/16 
  echo "Setup Latency: some l1x-node ip address: ${some_node_address}, mask: ${some_node_mask}*.*"
  local interface="$(ip --brief address show | gawk "/${some_node_mask}/{split(\$1, RES, \"@\"); print RES[1];}")"

  if [ ${base_latency} -le 20 ]; then
    jitter=2
  elif [ ${base_latency} -le 50 ]; then
    jitter=4
  else
    jitter=7
  fi

  # https://man7.org/linux/man-pages/man8/tc-netem.8.html
  echo "Setup random latency, interface: ${interface}, base_latency=${base_latency}ms, jitter=${jitter}ms, correlation=90%"
  sudo tc qdisc add dev ${interface} root netem delay ${base_latency}ms ${jitter}ms 90%

  # ping l1x-node1
}

setup_latency "${BASE_LATENCY}"

echo "Start delay: ${START_DELAY} sec"
sleep ${START_DELAY}

# Create a new directory for the random folder inside l1x_data
mkdir -p "l1x_data/$CHAIN_DATA"

# Run the server init command with the new directory inside l1x_data
RUST_LOG=info /usr/local/bin/server init -w "/l1x/l1x-consensus/l1x_data/$CHAIN_DATA"

RUST_LOG=info /usr/local/bin/server add-system-block -w "/l1x/l1x-consensus/l1x_data/$CHAIN_DATA"

# Run the server start command with the same directory
RUST_LOG=info /usr/local/bin/server start -w "/l1x/l1x-consensus/l1x_data/$CHAIN_DATA"