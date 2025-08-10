require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

/** @type import('hardhat/config').HardhatUserConfig */
module.exports = {
  solidity: {
    version: "0.8.19",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200
      }
    }
  },
  networks: {
    irysTestnet: {
      url: "https://testnet-rpc.irys.xyz/v1/execution-rpc",
      chainId: 1270,
      accounts: process.env.PRIVATE_KEY ? [process.env.PRIVATE_KEY] : [],
      gasPrice: 200000000000, // 20 gwei
      gas: 3000000,
      timeout: 60000, 
      httpHeaders: {
        "Content-Type": "application/json"
      }
    },
    localhost: {
      url: "http://127.0.0.1:8545",
      chainId: 31337
    }
  },
  etherscan: {
    apiKey: {
      irysTestnet: "no-api-key-needed"
    },
    customChains: [
      {
        network: "irysTestnet",
        chainId: 1270,
        urls: {
          apiURL: "https://explorer.irys.xyz/api",
          browserURL: "https://explorer.irys.xyz"
        }
      }
    ]
  },
  paths: {
    sources: "./contracts",
    tests: "./test",
    cache: "./cache",
    artifacts: "./artifacts"
  }
}; 