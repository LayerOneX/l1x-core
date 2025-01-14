export PRIV_KEY=f6b82b53ecbe1978b8651f740739b1d181f0285381e65e5e3491d8e821ab9bd0


../target/debug/cli --endpoint http://54.214.8.200:50051  --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/balancer-pool/pool_balancer_deployment.json

../target/debug/cli --endpoint http://54.214.8.200:50051  --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/balancer-pool/source_registry_deployment.json

../target/debug/cli --endpoint http://54.214.8.200:50051  --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/balancer-pool/source_registry_init.json

Pool Balancer (Initialized): e1459f3fdaeb2b7c30da2191e02d50f09d9f25b8
Source Registry (Initialized): 3db5eab26e452963c52b2d718930cc15c20d5382