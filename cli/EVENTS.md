This is description of the research in Sputnik EVM event publishing/subscription.

Firstly, deploy [event-example.sol](sol/event-example.sol), in a compiled version [event-example.compiled](sol/event-example.compiled):

```sh
cd $ROOT/cli
export RUST_LOG=info
export PRIV_KEY=6d657bbe6f7604fb53bc22e0b5285d3e2ad17f64441b2dc19b648933850f9b46
../target/debug/cli --private-key $PRIV_KEY submit-sol --payload-file-path txn-payload/evm_event_sample_deployment.json
```

Obtain from output the `contract_address`, paste it into [txn-payload/evm_event_sample_function_call.json](txn-payload/evm_event_sample_function_call.json)';s `contract_instance_address.hex`. Run the `helloWorld()` function call. Note, `c605f76c` corresponds to first 4 bytes of the Keccak-256 encoded hash: https://emn178.github.io/online-tools/keccak_256.html

```sh
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/evm_event_sample_function_call.json
```

Observe in server logs:

```
 INFO  contract_instance::contract_instance_evm > Handler: log: event_data: "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0 \0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\rHello, world!\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
 INFO  contract_instance::contract_instance_evm > Handler: log: topics: [0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80, 0x00000000000000000000000075104938baa47c54a86004ef998cc76c2e616289]
```

First log exposes `Hello, world!` event body, albeit with prefixed/suffixed `\0`s. Note, there's also a white space in there. I'm not sure what the format should be, what's the significance of the `\0`s and the whitespace is...

Second log lists topics to this event is published to:

- 0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80 - corresponds to Keccak-2556 encoded `NewMessage(address,string)`
- 0x00000000000000000000000075104938baa47c54a86004ef998cc76c2e616289 - corresponds to the indexed address (from event signature), as per server log

```
INFO  validate::validate_common > Address: "75104938baa47c54a86004ef998cc76c2e616289"
```

I believe topics are:

1. hashed event signature
2. ..n. value for given indexed param, in their declaration order. An event subscription must at minimum match the signature, potentially further indexed variables. Note, null subscription for a given filter means `do not filter`

For subscription implementation (to be done by us), I believe we'll need to Keccak-256 event signatures, and any indexed fields of interest. When event is received - match against those topics.

## Deploy an event emitting smart contract and monitor payloads via WS

Then get linea native tokens via: https://faucet.goerli.linea.build/. Note, use a wallet that has real eth on it (required). The tokens will go to goerli linea testnet. Switch to this testnet on metamask, to ensure subsequent deployments on that network.Ensure you have 0.5 LineaETH.

In Remix, Switch to Environment -> Injected Provider -> Metamask. Choose the metamask wallet with 0.5 LineaETH

Deploy [event-example.sol](sol/event-example.sol) via Remix to Linea Goerli, pay attention to contract address (eg. 0x537393A37A3BE4A8129E6cA9DAd8329BA6EEF228), monitor via https://explorer.goerli.linea.build/address/0x537393A37A3BE4A8129E6cA9DAd8329BA6EEF228/transactions#address-tabs

Given:

- contract_address = 0x537393A37A3BE4A8129E6cA9DAd8329BA6EEF228
- `NewMessage(address,string)` keccak 256 encoded - 0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80

Start a terminal connecting to linea goerli:

```sh
wscat -c wss://linea-goerli.infura.io/ws/v3/5b462c3977b64cf39f9c797764b9663c
{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["logs",{"fromBlock":"latest","toBlock":"latest","address":"0x537393A37A3BE4A8129E6cA9DAd8329BA6EEF228","topics":["0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80"]}]}
< {"jsonrpc":"2.0","id":1,"result":"0xc5bfa7001b444380bd3a443cf54241b3"}

# trigger "helloWorld" on remix, ensure it's gone through on https://explorer.goerli.linea.build/address/0x537393A37A3BE4A8129E6cA9DAd8329BA6EEF228/transactions#address-tabs
# expect an event!

< {"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"0xc5bfa7001b444380bd3a443cf54241b3","result":{"removed":false,"logIndex":"0x0","transactionIndex":"0x0","transactionHash":"0xe9f59f8260179eb41a01128dafc8fdb98436ae9d526499ba6ded59dba289d0de","blockHash":"0x17bd954495a07e0cba536eb64c2c7835d376ce824488572a3b8a90a07834e92e","blockNumber":"0x154427","address":"0x537393a37a3be4a8129e6ca9dad8329ba6eef228","data":"0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000d48656c6c6f2c20776f726c642100000000000000000000000000000000000000","topics":["0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80","0x0000000000000000000000002cbd9e754f49a520497ae12f7f407301ddf96ee9"]}}}
```

Note topics:

- 0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80 - `NewMessage(address,string)`
- 0x2cBD9e754f49A520497ae12F7f407301DDf96eE9 - is my metamask pubkey, ie. msg.sender address, which is indexed (hence available as topic)

## Monitor via typescript snippet [ts/subscribe_events.ts](ts/subscribe_events.ts)

```sh
cd sol/ts
npm i
./subscribe_events.ts
# trigger helloWorld from Remix, observe events
```

## Testing via ts script

```sh
export RUST_LOG=info
export PRIV_KEY=6d657bbe6f7604fb53bc22e0b5285d3e2ad17f64441b2dc19b648933850f9b46
../target/debug/cli --private-key $PRIV_KEY submit-sol --payload-file-path sol/txn-payload/evm_smart_contract_deployment.json
# Take the address from server logs, put into sol/txn-payload/evm_smart_contract_function_call.json:
"*******EXECUTING CONTRACT DEPLOYMENT******** : "80441bd7201609434b89e2712840cf88ef0c8ec2" "

# In second window:
cd sol/ts
./subscribe_events.ts

# In first window:
../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path sol/txn-payload/evm_smart_contract_function_call.json

```
