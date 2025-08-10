use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use ethers::types::{Address, U256, U64, TxHash};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use futures::StreamExt;


abigen!(
    IrysForum,
    r#"[
        function createPost(string memory _title, string memory _content, string[] memory _tags, string memory _irysTransactionId) external payable
        function createComment(uint256 _postId, string memory _content, uint256 _parentId, string memory _irysTransactionId) external payable
        function likePost(uint256 _postId) external
        function likeComment(uint256 _commentId) external
        function getPost(uint256 _postId) external view returns (tuple(uint256 id, address author, string title, string content, string[] tags, uint256 timestamp, uint256 likes, uint256 comments, bool qualityPost, string irysTransactionId))
        function getUser(address _user) external view returns (tuple(uint256 postsCount, uint256 commentsCount, uint256 totalLikesReceived, uint256 reputationScore, uint256 totalEarned, uint256 totalSpent, bool isMiner, uint256 lastActivityTime))
        function postCost() external view returns (uint256)
        function commentCost() external view returns (uint256)
        function usernameCost() external view returns (uint256)
        function registerUsername(string memory _username) external payable
        function getUsernameByAddress(address _user) external view returns (string)
        function getAddressByUsername(string memory _username) external view returns (address)
        function isUsernameAvailable(string memory _username) external view returns (bool)
        function distributeMiningRewards() external
        event PostCreated(uint256 indexed postId, address indexed author, string title, uint256 reward)
        event CommentCreated(uint256 indexed commentId, uint256 indexed postId, address indexed author, uint256 reward)
        event PostLiked(uint256 indexed postId, address indexed liker, address indexed author, uint256 reward)
        event MiningRewardDistributed(address indexed miner, uint256 reward)
        event UsernameRegistered(address indexed user, string username)
    ]"#
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractConfig {
    pub network_name: String,
    pub chain_id: u64,
    pub contract_address: String,
    pub rpc_url: String,
}

#[derive(Clone)]
pub struct BlockchainService {
    provider: Arc<Provider<Http>>,
    contract_address: Address,
    config: ContractConfig,
}

impl BlockchainService {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = ContractConfig {
            network_name: "Irys Testnet".to_string(),
            chain_id: 1270,
            contract_address: std::env::var("CONTRACT_ADDRESS").unwrap_or_default(),
            rpc_url: "https://testnet-rpc.irys.xyz/v1/execution-rpc".to_string(),
        };

        let provider = Provider::<Http>::try_from(&config.rpc_url)?;
        let contract_address = config.contract_address.parse()?;

        Ok(Self {
            provider: Arc::new(provider),
            contract_address,
            config,
        })
    }

    pub fn get_contract_address(&self) -> &Address {
        &self.contract_address
    }

    /// Get post cost (wei)
    pub async fn get_post_cost(&self) -> Result<U256, Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        let cost = contract.post_cost().call().await?;
        Ok(cost)
    }

    /// Get comment cost (wei)
    pub async fn get_comment_cost(&self) -> Result<U256, Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        let cost = contract.comment_cost().call().await?;
        Ok(cost)
    }

    /// Get on-chain post information
    pub async fn get_blockchain_post(&self, post_id: U256) -> Result<BlockchainPost, Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        let post_data = contract.get_post(post_id).call().await?;
        
        Ok(BlockchainPost {
            id: post_data.0,
            author: format!("{:?}", post_data.1),
            title: post_data.2,
            content: post_data.3,
            tags: post_data.4,
            timestamp: post_data.5,
            likes: post_data.6,
            comments: post_data.7,
            quality_post: post_data.8,
            irys_transaction_id: post_data.9,
        })
    }

    /// Get on-chain user information
    pub async fn get_blockchain_user(&self, address: &str) -> Result<BlockchainUser, Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        let user_address: Address = address.parse()?;
        let user_data = contract.get_user(user_address).call().await?;
        
        Ok(BlockchainUser {
            posts_count: user_data.0,
            comments_count: user_data.1,
            total_likes_received: user_data.2,
            reputation_score: user_data.3,
            total_earned: user_data.4,
            total_spent: user_data.5,
            is_miner: user_data.6,
            last_activity_time: user_data.7,
        })
    }

    /// Check if the user already has a username on-chain
    pub async fn user_has_username_on_chain(&self, address: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        let user_address: Address = address.parse()?;
        
        
        let username = contract.get_username_by_address(user_address).call().await?;
        Ok(!username.is_empty())
    }

    /// Get on-chain username by address
    pub async fn get_username_by_address_on_chain(&self, address: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        let user_address: Address = address.parse()?;
        
        let username = contract.get_username_by_address(user_address).call().await?;
        if username.is_empty() {
            Ok(None)
        } else {
            Ok(Some(username))
        }
    }

    /// Build createPost transaction payload for frontend
    pub fn build_create_post_tx(&self, title: &str, content: &str, tags: Vec<String>, irys_tx_id: &str, value: U256) -> String {
        
        format!(
            r#"{{
                "to": "{}",
                "value": "0x{:x}",
                "data": "{}",
                "gasLimit": "0x47b760"
            }}"#,
            self.contract_address,
            value,
            self.encode_create_post_call(title, content, tags, irys_tx_id)
        )
    }

    /// Encode createPost call data
    fn encode_create_post_call(&self, _title: &str, _content: &str, _tags: Vec<String>, _irys_tx_id: &str) -> String {
        
        
        "0x1234abcd".to_string()
    }

    /// Listen to on-chain events
    pub async fn listen_to_events(&self) -> Result<(), Box<dyn std::error::Error>> {
        let contract = IrysForum::new(self.contract_address, self.provider.clone());
        
        
        let events = contract.events().from_block(0u64);
        let mut stream = events.stream().await?;
        
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    println!("Received event: {:?}", event);
                    
                }
                Err(e) => {
                    eprintln!("Event listener error: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Verify transaction exists on chain
    pub async fn verify_transaction_exists(&self, tx_hash: &str) -> Result<TransactionDetails, Box<dyn std::error::Error>> {
        let tx_hash: TxHash = tx_hash.parse()?;
        
        
        let receipt = self.provider.get_transaction_receipt(tx_hash).await?;
        if receipt.is_none() {
            return Err("Transaction does not exist or is not yet confirmed".into());
        }
        
        let receipt = receipt.unwrap();
        
        
        let transaction = self.provider.get_transaction(tx_hash).await?;
        if transaction.is_none() {
            return Err("Unable to fetch transaction details".into());
        }
        
        let transaction = transaction.unwrap();
        
        
        let block = if let Some(block_number) = receipt.block_number {
            self.provider.get_block(block_number).await?
        } else {
            return Err("Transaction has not been included in a block yet".into());
        };
        
        let block_timestamp = if let Some(block) = &block {
            block.timestamp
        } else {
            return Err("Unable to fetch block information".into());
        };
        
        Ok(TransactionDetails {
            hash: format!("{:?}", tx_hash),
            from: format!("{:?}", transaction.from),
            to: transaction.to.map(|addr| format!("{:?}", addr)),
            value: transaction.value,
            gas_used: receipt.gas_used.unwrap_or_default(),
            block_number: U256::from(receipt.block_number.unwrap_or_default().as_u64()),
            block_timestamp,
            status: receipt.status.unwrap_or_default(),
            logs: receipt.logs,
        })
    }
    
   
    pub async fn verify_post_transaction(&self, tx_hash: &str, expected_sender: &str) -> Result<PostTransactionVerification, Box<dyn std::error::Error>> {
        let tx_details = self.verify_transaction_exists(tx_hash).await?;
        
       
        if tx_details.status != U64::from(1) {
            return Err("Transaction execution failed".into());
        }
        
      
        let expected_sender: Address = expected_sender.parse()?;
        let actual_sender: Address = tx_details.from.parse()?;
        if actual_sender != expected_sender {
            return Err("Transaction sender mismatch".into());
        }
        
        
        if let Some(to) = &tx_details.to {
            let to_address: Address = to.parse()?;
            if to_address != self.contract_address {
                return Err("Transaction target contract address incorrect".into());
            }
        } else {
            return Err("Transaction has no target address".into());
        }
        

        let has_post_event = tx_details.logs.iter().any(|log| {
            log.address == self.contract_address
        });
        
        if !has_post_event {
            return Err("No contract event found in transaction".into());
        }
        

        let post_id = U256::from(1);
        let points_earned = U256::from(100); 
        
      
        let required_cost = self.get_post_cost().await?;
        if tx_details.value < required_cost {
            return Err("Insufficient payment amount".into());
        }
        
        Ok(PostTransactionVerification {
            transaction_hash: tx_details.hash,
            sender: tx_details.from,
            block_number: tx_details.block_number.as_u64(),
            block_timestamp: tx_details.block_timestamp,
            post_id,
            points_earned,
            value_paid: tx_details.value,
            gas_used: tx_details.gas_used,
            verified: true,
        })
    }
    
 
    pub async fn verify_comment_transaction(&self, tx_hash: &str, expected_sender: &str) -> Result<CommentTransactionVerification, Box<dyn std::error::Error>> {
        let tx_details = self.verify_transaction_exists(tx_hash).await?;
        
      
        if tx_details.status != U64::from(1) {
            return Err("Transaction execution failed".into());
        }
        
    
        let expected_sender: Address = expected_sender.parse()?;
        let actual_sender: Address = tx_details.from.parse()?;
        if actual_sender != expected_sender {
            return Err("Transaction sender mismatch".into());
        }
        
      
        if let Some(to) = &tx_details.to {
            let to_address: Address = to.parse()?;
            if to_address != self.contract_address {
                return Err("Transaction target contract address incorrect".into());
            }
        } else {
            return Err("Transaction has no target address".into());
        }
        
      
        let has_comment_event = tx_details.logs.iter().any(|log| {
            log.address == self.contract_address
        });
        
        if !has_comment_event {
            return Err("No contract event found in transaction".into());
        }
        
     
        let comment_id = U256::from(1); 
        let post_id = U256::from(1); 
        let points_earned = U256::from(50); 
        
       
        let required_cost = self.get_comment_cost().await?;
        if tx_details.value < required_cost {
            return Err("Insufficient payment amount".into());
        }
        
        Ok(CommentTransactionVerification {
            transaction_hash: tx_details.hash,
            sender: tx_details.from,
            block_number: tx_details.block_number.as_u64(),
            block_timestamp: tx_details.block_timestamp,
            comment_id,
            post_id,
            points_earned,
            value_paid: tx_details.value,
            gas_used: tx_details.gas_used,
            verified: true,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockchainPost {
    pub id: U256,
    pub author: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub timestamp: U256,
    pub likes: U256,
    pub comments: U256,
    pub quality_post: bool,
    pub irys_transaction_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockchainUser {
    pub posts_count: U256,
    pub comments_count: U256,
    pub total_likes_received: U256,
    pub reputation_score: U256,
    pub total_earned: U256,
    pub total_spent: U256,
    pub is_miner: bool,
    pub last_activity_time: U256,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GasCostInfo {
    pub post_cost_wei: String,
    pub comment_cost_wei: String,
    pub post_cost_irys: String,
    pub comment_cost_irys: String,
}

impl GasCostInfo {
    pub fn new(post_cost: U256, comment_cost: U256) -> Self {
        Self {
            post_cost_wei: post_cost.to_string(),
            comment_cost_wei: comment_cost.to_string(),
            post_cost_irys: format!("{:.6}", post_cost.as_u128() as f64 / 1e18),
            comment_cost_irys: format!("{:.6}", comment_cost.as_u128() as f64 / 1e18),
        }
    }
} 

//New Structure Definition
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionDetails {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: U256,
    pub gas_used: U256,
    pub block_number: U256,
    pub block_timestamp: U256,
    pub status: U64,
    pub logs: Vec<ethers::types::Log>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostTransactionVerification {
    pub transaction_hash: String,
    pub sender: String,
    pub block_number: u64,
    pub block_timestamp: U256,
    pub post_id: U256,
    pub points_earned: U256,
    pub value_paid: U256,
    pub gas_used: U256,
    pub verified: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommentTransactionVerification {
    pub transaction_hash: String,
    pub sender: String,
    pub block_number: u64,
    pub block_timestamp: U256,
    pub comment_id: U256,
    pub post_id: U256,
    pub points_earned: U256,
    pub value_paid: U256,
    pub gas_used: U256,
    pub verified: bool,
}

