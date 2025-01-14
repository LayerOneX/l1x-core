# Install dependecies

These tests use `cargo-l1x`. Need to installs all dependecies that are required by `cargo-l1x`: https://crates.io/crates/cargo-l1x

# Integration Tests for Consensus

This test suite is designed to verify the basic functionality of the consensus system by testing various transactions, such as deploying contracts, calling contracts, and more, on a running node instance.

## Prerequisites

To run these integration tests, you need to have a consensus instance running and accessible. The tests will interact with this instance to perform the necessary operations.

## Configuration

The `config.toml` file contains all the configuration settings required to run the tests. You can modify this file to match your setup.

The following fields can be edited:

- `private_key_1` and `private_key_2`: Private keys of the accounts used in the tests. These keys should have sufficient funds to perform the necessary operations.
- `grpc_endpoint`: The URL of the gRPC endpoint of the consensus instance where you want to run the tests.

## Running the Tests

To run the integration tests, navigate to the project directory and execute the following command:

```bash
cargo test -- --test-threads=1
```

This command will compile and run all the integration tests defined in the `integration_tests` module.

## Test Coverage

The integration tests cover the following scenarios:

- Deploying and initiating a new smart contract
- Calling a function on a deployed contract
- Making a readonly call to a contract function
- Transferring tokens between accounts
- Verifying account balances and contract state

Please note that these tests interact with a live consensus instance and may have side effects, such as deploying contracts or transferring tokens. Make sure to use a dedicated testing environment or take necessary precautions to avoid unintended consequences.
