import { HardhatUserConfig } from "hardhat/config";
import "@nomicfoundation/hardhat-toolbox";

const GOERLI_API_KEY = "AIs4eRUD6ZfFZr98L8K3XP5IR86wfC-D";
const SEPOLIA_API_KEY = "hRIeBmj3fWdHkWc_sLA4MD-zfuj4I9sP;"
const LINNEA_GOERLI_API_KEY = "5b462c3977b64cf39f9c797764b9663c";
const PRIVATE_KEY = "cbee1efe577ab28c10101ce05320d6b2f29f21d0c8e2b5dfc650d8900371c073";

const config: HardhatUserConfig = {
  solidity: "0.8.19",
  networks: {
    localhost: {
      url: `http://127.0.0.1:50051`,
      accounts: [PRIVATE_KEY],
    },
    goerli: {
      url: `https://eth-goerli.g.alchemy.com/v2/${GOERLI_API_KEY}`,
      accounts: [PRIVATE_KEY],
    },
    sepolia: {
      url: `https://sepolia.infura.io/v3/d63ab1fa2f0b4f6cac1158aa30dd5130`,
      accounts: [PRIVATE_KEY],
    },
    linea_goerli: {
      url: `https://linea-goerli.infura.io/ws/v3/${LINNEA_GOERLI_API_KEY}`,
      accounts: [PRIVATE_KEY],
    },
  },
};

export default config;
