../target/debug/cli --endpoint http://18.237.195.130:50051 --private-key $PRIV_KEY submit-sol --payload-file-path txn-payload/evm_smart_contract_deployment.json

# Take the address from server logs:
"*******EXECUTING EVM CONTRACT DEPLOYMENT******** : "80441bd7201609434b89e2712840cf88ef0c8ec2" "

../target/debug/cli --endpoint http://18.237.195.130:50051 --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/evm_smart_contract_function_call.json


# evm smart contract related, pass in contract_instance_address to function_call. Note, there's no "init" step
../target/debug/cli --private-key $PRIV_KEY submit-sol --payload-file-path txn-payload/evm_smart_contract_deployment.json
# Take the address from server logs:
"*******EXECUTING EVM CONTRACT DEPLOYMENT******** : 80441bd7201609434b89e2712840cf88ef0c8ec2 "



../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/pool_stable/pool_stable_create.json

../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/pool_stable/pool_stable_join.json

../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/pool_stable/pool_stable_exit.json

../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/pool_stable/pool_stable_swap.json


# Server Details:

http://18.237.195.130:50051
18.237.195.130

- Fix Permission
chmod 600 l1x-cm.pem

- Connect to Server:
ssh -o StrictHostKeyChecking=no -i l1x-cm.pem ubuntu@18.237.195.130

- After Server Login > To View L1X-Node Logs:
docker logs -f l1x-consensus_l1x-node_1

- After Server Login > To View Cassandra Logs:
docker logs -f l1x-consensus_cassandra_1


-----------------

getPoolId() -> 38fff2d0
getBalance() -> 18160ddd