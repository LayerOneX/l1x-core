#!/bin/bash

# Loop 5 times, for example
for i in {1..5}; do
    ../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/native_token_transfer.json
    echo "Exec $i time(s)"
done