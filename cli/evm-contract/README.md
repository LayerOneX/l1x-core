# Sample Hardhat Project

This project demonstrates a basic Hardhat use case. It comes with a sample contract, a test for that contract, and a script that deploys that contract.

Try running some of the following tasks:

```shell
npx hardhat help
npx hardhat test
REPORT_GAS=true npx hardhat test
npx hardhat node
npx hardhat run scripts/deploy.ts
```

# Deployment

Deploy to Goerli:

```sh
npx hardhat run scripts/deploy.ts --network goerli
> Compiled 1 Solidity file successfully
> Lock with 0.001ETH and unlock timestamp 1693460699 deployed to 0x7B24dA52F729f4498923C44F00dbfF59c8078306
```

Deploy to local L1X:

```sh
# start L1X node
cd $ROOT
RUST_LOG=info cargo run --bin server -- --dev

# deploy
npx hardhat run scripts/deploy.ts --network localhost
> Lock with 0.001ETH and unlock timestamp 1695078598 deployed to 0x2fb1409ba26be0d556F37FE7f7794144f989e774
```
