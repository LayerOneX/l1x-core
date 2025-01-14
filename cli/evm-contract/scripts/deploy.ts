import fs from 'fs';
import path from 'path';
import dotenv from 'dotenv';

dotenv.config();

async function main() {
  const { ethers } = require('hardhat');
    // Replace with your contract name
    const contractName = process.env.CONTRACT_NAME;
    if (!contractName) {
      throw new Error("Contract name not found in environment variables");
    }
    const artifactsPath = `../../artifacts/${contractName}.json` // Change this for different path
    console.log(`Reading contract artifacts from ${artifactsPath}`);
    const metadata = JSON.parse(fs.readFileSync(artifactsPath, 'utf-8'));
    
    const privateKey = process.env.SIGNER;
    if (!privateKey) {
      throw new Error("Private key not found in environment variables");
    }

    const sleep = (ms: number) => {
      return new Promise((resolve) => setTimeout(resolve, ms));
    };

    // Fetch the signer options and args from the environment variables
    const signer = new ethers.Wallet(privateKey, ethers.provider);
    const args = JSON.parse(process.env.ARGS || '[]');

    // Retrieve the compiled contract artifacts
    const factory = new ethers.ContractFactory(metadata.abi, metadata.bytecode, signer);

    // Deploy the contract with the provided signer options and args
    const contract = await factory.deploy(...args);
    
    await sleep(5000);

    console.log(`${contractName} deployed to:`, await contract.address);
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main()
    .then(() => process.exit(0))
    .catch((error) => {
        console.error(error);
        process.exit(1);
    });
