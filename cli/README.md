# L1X command line utility

Utility to provide CLI commands to the node.

## Setup

Cassandra reset/(re)start - perform reset when need to clear out tables.

```sh
# reset
docker kill l1x-cassandra && docker rm l1x-cassandra && docker container prune -f && docker volume prune -f
# run cassandra
docker run --name l1x-cassandra -d -p 9042:9042 -e CASSANDRA_CLUSTER_NAME=l1x-cassandra cassandra
```

Start a node(/rpc_json):

### add this environment variable before running 
``` 
export RUST_LOG=info
export REPLICATION_ENABLED=false  # to run a single node locally without cassandra
```

* Replication requires if you have setup cassandra snitch in your environment

## How to run node

- init command which creates genesis.json and config.toml file in home_directory.

```bash
./target/debug/server init
```

- start command which reads genesis.json and config.toml file from home_directory.

```bash
RUST_LOG=info ./target/debug/server start
```
## CLI usage

```sh
export RUST_LOG=info
export PRIV_KEY=6913aeae91daf21a8381b1af75272fe6fae8ec4a21110674815c8f0691e32758

# prints options
../target/debug/cli

../target/debug/cli payload-examples

# Getter examples
../target/debug/cli --private-key $PRIV_KEY account-state -a 75104938baa47c54a86004ef998cc76c2e616289
../target/debug/cli --rpc-type json --private-key $PRIV_KEY transaction-receipt --hash 6427c422661cf8cea0547a11ebb811813704dfd5c3ea934b60192bd9f74eaa2b
../target/debug/cli --rpc-type json --private-key $PRIV_KEY block-by-number --block-number 1400
../target/debug/cli --private-key $PRIV_KEY transactions-by-account -a 75104938baa47c54a86004ef998cc76c2e616289 --number-of-transactions 3 --starting-from 0
../target/debug/cli --private-key $PRIV_KEY get-stake --pool-address 522b3294fe78d57a1d7e1c37393f11841f6a9494 --account-address 50028cf7ed245e4ac9e472d5277f14ed1c7ab384

# Specifying a different endpoint (for development)
../target/debug/cli --private-key $PRIV_KEY --endpoint http://127.0.0.1:50055 account-state -a 137da5cfffa18adb4bd4ae4248dd9aa9355310a3

# submit transactions, requires creation of a payload inside config json file
# note, node address currently hard coded to [2u8; 20]
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/native_token_transfer.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/stake.json

# smart contract related, pass in contract_address to init, contract_instance_address to function_call
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/smart_contract_deployment.json
# Take the address from server logs:
"*******EXECUTING CONTRACT DEPLOYMENT******** : "80441bd7201609434b89e2712840cf88ef0c8ec2" "

../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/smart_contract_init.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/smart_contract_function_call.json

# Read only call
../target/debug/cli --private-key $PRIV_KEY read-only-func-call --payload-file-path txn-payload/smart_contract_read_only_call.json

#Example on get price function:
../target/debug/cli --private-key $PRIV_KEY get-price --payload-file-path txn-payload/ft_smartcontract/deployment.json

# For example, with the L1X Fungible Token smart contract
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/ft_smartcontract/deployment.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/ft_smartcontract/init.json
../target/debug/cli --private-key $PRIV_KEY read-only-func-call --payload-file-path txn-payload/ft_smartcontract/ft_name.json

# xtalk smart contract related, pass in contract_address to init, contract_instance_address to function_call
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/xtalk_smart_contract_deployment.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/xtalk_smart_contract_init.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/xtalk_smart_contract_function_call.json

# evm smart contract related, pass in contract_instance_address to function_call. Note, there's no "init" step
../target/debug/cli --private-key $PRIV_KEY submit-sol --payload-file-path txn-payload/evm_smart_contract_deployment.json
# Take the address from server logs:
"*******EXECUTING EVM CONTRACT DEPLOYMENT******** : "80441bd7201609434b89e2712840cf88ef0c8ec2" "
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/evm_smart_contract_function_call.json


# staking related, pass the pool address to stake/un_stake
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/create_staking_pool.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/stake.json
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/un_stake.json
```