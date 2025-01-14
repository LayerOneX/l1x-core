Files in `contract_binaries` are built from https://github.com/L1X-Foundation-Consensus/l1x-system-contracts
The binaries versions can be checked using:
```bash
strings system_contracts/contracts_binaries/system_config.o | grep git:
```
Output will be like the following:
```
61e8279v0.1.0 git:
```
Each contract has `version()` method which returns the contract version in the following format:
```
v0.1.0 git:61e8279
```