#!ts-node

// Run with:
// ```sh
// npm i
// ./subscribe_events.ts
// ```
// Equivalent to scanning with wscat
// ```sh
// wscat -c wss://linea-goerli.infura.io/ws/v3/5b462c3977b64cf39f9c797764b9663c
// > {"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["logs",{"fromBlock":"latest","toBlock":"latest","address":"0x537393A37A3BE4A8129E6cA9DAd8329BA6EEF228","topics":["0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80"]}]}
// < {"jsonrpc":"2.0","id":1,"result":"0xc5bfa7001b444380bd3a443cf54241b3"}
//
// < {"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"0xc5bfa7001b444380bd3a443cf54241b3","result":{"removed":false,"logIndex":"0x0","transactionIndex":"0x0","transactionHash":"0xe9f59f8260179eb41a01128dafc8fdb98436ae9d526499ba6ded59dba289d0de","blockHash":"0x17bd954495a07e0cba536eb64c2c7835d376ce824488572a3b8a90a07834e92e","blockNumber":"0x154427","address":"0x537393a37a3be4a8129e6ca9dad8329ba6eef228","data":"0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000d48656c6c6f2c20776f726c642100000000000000000000000000000000000000","topics":["0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80","0x0000000000000000000000002cbd9e754f49a520497ae12f7f407301ddf96ee9"]}}}
import { ethers } from "ethers";
const CONTRACT_ADDRESS = require("../txn-payload/evm_smart_contract_function_call.json")["smart_contract_function_call"]["contract_instance_address"]["hex"];
const CONTRACT_ABI = require("../event-example.abi.json");
// const CONTRACT_ABI = require("../event-example.abi");
console.log(CONTRACT_ABI);
console.log(CONTRACT_ADDRESS);
// const WS_URL = "wss://linea-goerli.infura.io/ws/v3/5b462c3977b64cf39f9c797764b9663c";
const WS_URL = "ws://127.0.0.1:50051";

const senderPubkey = "75104938baa47c54a86004ef998cc76c2e616289"; // "0215edb7e9a64f9970c60d94b866b73686980d734874382ad1002700e5d870d945";
const senderAddress = "0x" + senderPubkey.slice(0, 40);
const senderAddressHex = ethers.utils.hexZeroPad(senderAddress, 32);
const bogusAddressHex = ethers.utils.hexZeroPad("0x" + "9999999999999999999999999999999999999999".slice(0, 40), 32);
const number10 = ethers.utils.hexZeroPad("0xa", 32);
const number50 = ethers.utils.hexZeroPad("0x32", 32);

const provider = new ethers.providers.WebSocketProvider(WS_URL);
const signer = provider.getSigner();
const contract = new ethers.Contract(CONTRACT_ADDRESS, CONTRACT_ABI, signer);

const filter1 = {
  topics: [ethers.utils.id("Event1(address,string,address,uint256)"), senderAddressHex, number10, null],
  // topics: [ethers.utils.id("Event1(address,string,address,uint256)")],
};
const filter2 = {
  topics: [ethers.utils.id("Event2(address,string,uint256)"), senderAddressHex, number50, null],
  // topics: [ethers.utils.id("Event1(address,string,address,uint256)")],
};

console.log(`senderPubkey: ${senderPubkey}\nsenderAddress: ${senderAddress}\nsenderAddressHex: ${senderAddressHex}`);
console.log(`subscribing to: filter1: ${JSON.stringify(filter1)}`);
console.log(`subscribing to: filter2: ${JSON.stringify(filter2)}`);

// contract.on("Event1(address,string,address,uint)", (sender, message, sender2, val) => {
contract.on(filter1, (sender, message, receiver, val) => {
  console.log(`Event1 sender: ${sender}: message: ${message} receiver: ${receiver}: val: ${val}`);
});
contract.on(filter2, (sender, message, val) => {
  console.log(`Event2 sender: ${sender}: message: ${message} val: ${val}`);
});
