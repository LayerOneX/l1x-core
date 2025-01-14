### This is description of the how to interact with the hardhat interface.

Make sure you you have added all your contract files to the `evm-contract` folder inside the cli file. This should include the `deployment` and `configuration` files. If you have downloaded a folder from open zepplin, unzip the folder and copy all your inner files into the `evm-contract` folder. These are all cli commands so make sure you have properly compiled and are running the l1x node.

Below are the commands:

1. #### Start hardhat:
This command allows you to start a local hardhat node

```sh
 cargo run --bin cli start-hardhat
```

2. #### Compile contract:
The command allows you to compile a smart contract using hardhat

```sh
 cargo run --bin cli hardhat-compile 
```

3. #### Test the smart contract:
You can test your smart contracts before deployment

```sh
 cargo run --bin cli contract-hardhat-test 
```

4. #### Deploy contract: 
You can deploy your smart contracts to any network in your configuration

```sh
 cargo run --bin cli deploy-contract-hardhat --network <NETWORK> 
```

5. #### Verify contract hardhat:
This allows you to verify the deployed smart contract. The contructor argument is optional.

```sh
 cargo run bin cli cli verify-contract-hardhat --network <NETWORK> --address <ADDRESS> --constructor-args <ARGS> 
```

6. #### Flatten contract: 
You can flatten the smart contract code. This flattened code can be used during manual verification.

```sh
 cargo run bin cli cli hardhat-flatten 
```

7. #### Open local hardhat console:
You can open a local hardhat console. 

```sh
 cargo run bin cli open-hardhat-console 
```