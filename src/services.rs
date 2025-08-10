use crate::models::*;
use crate::blockchain::BlockchainService;
use crate::database::DatabaseService;
use chrono::Utc;
use log::info;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::Arc;
use sha2::{Sha256, Digest};
use uuid;

pub struct IrysService {
    client: Client,
    testnet_url: String,
    explorer_url: String,
}

impl IrysService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            testnet_url: "https://testnet-rpc.irys.xyz/v1/execution-rpc".to_string(),
            explorer_url: "https://explorer.irys.xyz".to_string(),
        }
    }

    pub async fn upload_data(&self, _data: &str, _tags: Vec<String>, _address: &str) -> Result<String, Box<dyn std::error::Error>> {
      
        let tx_id = format!("mock_tx_{}", chrono::Utc::now().timestamp_millis());
        info!("Mock Irys upload with transaction ID: {}", tx_id);
        Ok(tx_id)
        
        
        /*
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "irys_uploadData",
            "params": {
                "data": data,
                "tags": tags,
                "address": address
            },
            "id": 1
        });

        let response = self.client
            .post(&self.testnet_url)
            .json(&payload)
            .send()
            .await?;

        let result: Value = response.json().await?;
        
        if let Some(error) = result.get("error") {
            return Err(format!("Irys upload error: {}", error).into());
        }

        if let Some(result_data) = result.get("result") {
            if let Some(tx_id) = result_data.get("transactionId") {
                if let Some(tx_id_str) = tx_id.as_str() {
                    info!("Successfully uploaded to Irys with transaction ID: {}", tx_id_str);
                    return Ok(tx_id_str.to_string());
                }
            }
        }

        Err("Failed to get transaction ID from response".into())
        */
    }

    pub async fn query_data(&self, address: Option<&str>, tags: Option<Vec<String>>, limit: Option<u32>) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let mut params = HashMap::new();
        
        if let Some(addr) = address {
            params.insert("address".to_string(), addr.to_string());
        }
        
        if let Some(tags) = tags {
            let tags_str = serde_json::to_string(&tags)?;
            params.insert("tags".to_string(), tags_str);
        }
        
        if let Some(limit) = limit {
            let limit_str = limit.to_string();
            params.insert("limit".to_string(), limit_str);
        }

        let query_string = serde_urlencoded::to_string(&params)?;
        let url = format!("{}/query?{}", self.explorer_url, query_string);

        let response = self.client
            .get(&url)
            .send()
            .await?;

        let result: Value = response.json().await?;
        
        if let Some(data) = result.get("data") {
            if let Some(transactions) = data.as_array() {
                return Ok(transactions.clone());
            }
        }

        Ok(Vec::new())
    }
}

pub struct ForumService {
    posts: Arc<Mutex<HashMap<String, Post>>>, 
    comments: Arc<Mutex<HashMap<String, Comment>>>, 
    users: Arc<Mutex<HashMap<String, User>>>, 
    irys_service: IrysService,
    blockchain_service: Option<BlockchainService>,
    database_service: Option<DatabaseService>,
    cache_service: Option<Arc<crate::cache::CacheService>>, 
    async_queue_service: Option<Arc<crate::async_queue::AsyncQueueService>>, 
}

impl ForumService {
    pub async fn new() -> Self {
        let blockchain_service = match BlockchainService::new() {
            Ok(service) => {
                info!("✅ Blockchain service initialization successful");
                Some(service)
            },
            Err(e) => {
                info!("⚠️Blockchain service initialization failed: {}, offline mode will be used", e);
                None
            }
        };

        let database_service = match std::env::var("DATABASE_URL") {
            Ok(database_url) => {
                match DatabaseService::new(&database_url).await {
                    Ok(service) => {
                        info!("✅ Database service initialization successful");
                        Some(service)
                    },
                    Err(e) => {
                        info!("⚠️ Database service initialization failed: {}, will use memory storage", e);
                        None
                    }
                }
            },
            Err(_) => {
                info!("⚠️ DATABASE-URL not set, memory storage will be used");
                None
            }
        };

        
        let cache_service = match std::env::var("REDIS_URL") {
            Ok(redis_url) => {
                match crate::cache::CacheService::new(&redis_url) {
                    Ok(service) => {
                        info!("✅ Redis cache service initialization successful: {}", redis_url);
                        Some(Arc::new(service))
                    },
                    Err(e) => {
                        info!("⚠️ Redis cache service initialization failed: {}, cache will not be used", e);
                        None
                    }
                }
            },
            Err(_) => {
                info!("⚠️ REDIS-URL not set, cache will not be used");
                None
            }
        };

        
        let async_queue_service = {
            let blockchain_service_arc = blockchain_service.as_ref().map(|s| Arc::new(s.clone()));
            let database_service_arc = database_service.as_ref().map(|s| Arc::new(s.clone()));
            
            let queue_service = crate::async_queue::AsyncQueueService::new(
                blockchain_service_arc,
                database_service_arc,
            );
            
            info!("✅ Asynchronous queue service initialization completed");
            Some(Arc::new(queue_service))
        };

        Self {
            posts: Arc::new(Mutex::new(HashMap::new())),
            comments: Arc::new(Mutex::new(HashMap::new())),
            users: Arc::new(Mutex::new(HashMap::new())),
            irys_service: IrysService::new(),
            blockchain_service,
            database_service,
            cache_service,
            async_queue_service,
        }
    }

    pub fn generate_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
    
    fn generate_content_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub async fn create_post(&self, request: CreatePostRequest) -> Result<Post, Box<dyn std::error::Error>> {
        let post_data = serde_json::to_string(&request)?;
        let tags = vec!["forum".to_string(), "post".to_string()];
        let author_address = request.author_address.clone();
        
       
        let author_name = if let Ok(Some(username)) = self.get_username_by_address(&author_address).await {
            Some(username)
        } else {
            request.author_name
        };
        
     
        let tx_id = self.irys_service.upload_data(&post_data, tags, &author_address).await?;

        
        if let Some(_blockchain_service) = &self.blockchain_service {
            info!("🔗 Blockchain service available - contract address: {}", std::env::var("CONTRACT_ADDRESS").unwrap_or_default());
            info!("📝 The post has been created, and the frontend can call the contract for on-chain recording");
            info!("💡 Parameters: title={}, tags={:?}, irys_tx={}", request.title, request.tags, tx_id);
        } else {
            info!("⚠️ Offline mode: Skipping blockchain integration");
        }

        let post = Post {
            id: Self::generate_id(),
            title: request.title,
            content: request.content,
            author_address: request.author_address,
            author_id: None, 
            author_name,
            author_avatar: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            likes: 0,
            comments_count: 0,
            tags: request.tags,
            irys_transaction_id: Some(tx_id),
            image: request.image,
            blockchain_post_id: request.blockchain_post_id,
            is_liked_by_user: false, 
            views: 0,
            heat_score: None, 
        };

        // Prioritize using database storage
        if let Some(db) = &self.database_service {
            match db.create_post(&post).await {
                Ok(_) => {
                    info!("📊 The post has been saved to the database: {}", post.id);
                    
                    
                    if let Some(cache) = &self.cache_service {
                        if let Err(e) = cache.invalidate_post_cache() {
                            info!("⚠️ Clearing post cache failed: {}", e);
                        } else {
                            info!("🗑️ Cleared post list cache");
                        }
                    }
                },
                Err(e) => {
                    info!("⚠️Database save failed, using memory storage: {}", e);
                   
                    self.posts.lock().unwrap().insert(post.id.clone(), post.clone());
                    self.update_user_stats(&author_address, true, false).await;
                }
            }
        } else {
       
            self.posts.lock().unwrap().insert(post.id.clone(), post.clone());
            self.update_user_stats(&author_address, true, false).await;
        }

        Ok(post)
    }

    pub async fn get_posts(&self) -> Vec<Post> {
        self.get_posts_paginated(1000, 0).await 
    }
    
   
    pub async fn get_posts_with_like_status(&self, user_address: Option<&str>) -> Vec<Post> {
        self.get_posts_paginated_with_like_status(1000, 0, user_address).await
    }
    
    pub async fn get_posts_paginated_with_like_status(&self, limit: u32, offset: u32, user_address: Option<&str>) -> Vec<Post> {
        
        let mut posts = self.get_posts_paginated(limit, offset).await;
        
      
        if let (Some(user_addr), Some(db)) = (user_address, &self.database_service) {
            for post in &mut posts {
                if let Ok(is_liked) = db.has_user_liked_post(&post.id, user_addr).await {
                    post.is_liked_by_user = is_liked;
                }
            }
        }
        
        posts
    }
    
    pub async fn get_posts_paginated(&self, limit: u32, offset: u32) -> Vec<Post> {
        
        if let Some(cache) = &self.cache_service {
            match cache.get_cached_posts(limit, offset) {
                Ok(Some(posts)) => {
                    info!("⚡ Retrieve {} posts from Redis cache (limit: {}, offset: {})", posts.len(), limit, offset);
                    return posts;
                },
                Ok(None) => {
                    info!("📭 Redis cache miss, querying database");
                },
                Err(e) => {
                    info!("⚠️ Redis cache query failed: {}", e);
                }
            }
        }

    
        if let Some(db) = &self.database_service {
            match db.get_posts_paginated(limit, offset).await {
                Ok(posts) => {
                    info!("📊 Retrieved {} posts from the database (limit: {}, offset: {})", posts.len(), limit, offset);
                    
                    
                    if let Some(cache) = &self.cache_service {
                        if let Err(e) = cache.cache_posts(&posts, limit, offset) {
                            info!("⚠️ Cache post failed: {}", e);
                        } else {
                            info!("💾 The post has been cached Redis");
                        }
                    }
                    
                    return posts;
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in memory data: {}", e);
                }
            }
        }
        
    
        let posts = self.posts.lock().unwrap();
        let mut post_list: Vec<Post> = posts.values().cloned().collect();
        post_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
    
        let start = offset as usize;
        let end = (start + limit as usize).min(post_list.len());
        let paginated_posts = if start < post_list.len() {
            post_list[start..end].to_vec()
        } else {
            Vec::new()
        };
        
        info!("📊 Retrieved {} posts from memory (limit: {}, offset: {})", paginated_posts.len(), limit, offset);
        paginated_posts
    }

    pub async fn get_post(&self, id: &str) -> Option<Post> {
     
        if let Some(db) = &self.database_service {
            match db.get_post_by_id(id).await {
                Ok(Some(post)) => {
                    info!("📊 Retrieved post from the database: {}", id);
                    return Some(post);
                },
                Ok(None) => {
                    info!("📊 Post not found in database: {}", id);
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in memory data: {}", e);
                }
            }
        }
        
       
        let posts = self.posts.lock().unwrap();
        posts.get(id).cloned()
    }

    pub async fn get_post_with_like_status(&self, id: &str, user_address: Option<&str>) -> Option<Post> {
        if let Some(db) = &self.database_service {
            match db.get_post_by_id_with_like_status(id, user_address).await {
                Ok(Some(post)) => {
                    info!("📊 Retrieved post (including like status) from the database: {}", id);
                    return Some(post);
                },
                Ok(None) => {
                    info!("📊 No post found in the database: {}", id);
                },
                Err(e) => {
                    info!("⚠️ Database query failed, fallback to no like status: {}", e);
                }
            }
        }
        
      
        self.get_post(id).await
    }

    pub async fn add_comment(&self, request: CreateCommentRequest) -> Result<Comment, Box<dyn std::error::Error>> {
        let comment_data = serde_json::to_string(&request)?;
        let tags = vec!["forum".to_string(), "comment".to_string()];
        let author_address = request.author_address.clone();
        let post_id = request.post_id.clone();
        
       
        let author_name = if let Ok(Some(username)) = self.get_username_by_address(&author_address).await {
            Some(username)
        } else {
            request.author_name
        };
        
       
        let tx_id = self.irys_service.upload_data(&comment_data, tags, &author_address).await?;

        let comment = Comment {
            id: Self::generate_id(),
            post_id: request.post_id.clone(),
            content: request.content.clone(),
            author_address: request.author_address.clone(),
            author_id: None, 
            author_name: author_name.clone(),
            author_avatar: None,
            created_at: Utc::now(),
            parent_id: request.parent_id.clone(),
            likes: 0,
            irys_transaction_id: Some(tx_id),
            image: request.image.clone(),
            content_hash: Self::generate_content_hash(&request.content),
            is_liked_by_user: false,
        };

        
        if let Some(db) = &self.database_service {
            match db.create_comment(&comment).await {
                Ok(_) => {
                    info!("🔗 Blockchain service available - contract address: {:?}", 
                          self.blockchain_service.as_ref().map(|s| s.get_contract_address()));
                    info!("📝 The comment has been created, and the frontend can call the contract for on chain recording");
                    info!("💡parameter: content={}, post_id={}", comment.content, comment.post_id);
                    
                
                    if let Some(cache) = &self.cache_service {
                        if let Err(e) = cache.invalidate_comment_cache(&comment.post_id) {
                            info!("⚠️ Clearing comment cache failed: {}", e);
                        } else {
                            info!("🗑️ Cleared post comment cache");
                        }
                    }
                    
                    return Ok(comment);
                },
                Err(e) => {
                    info!("⚠️ Database save failed, using memory storage: {}", e);
                }
            }
        }

     
        let comment_id = comment.id.clone();
        self.comments.lock().unwrap().insert(comment_id.clone(), comment.clone());
        
    
        if let Some(post) = self.posts.lock().unwrap().get_mut(&post_id) {
            post.comments_count += 1;
        }
        
      
        self.update_user_stats(&author_address, false, true).await;

        Ok(comment)
    }

    pub async fn get_comments(&self, post_id: &str) -> Result<Vec<Comment>, Box<dyn std::error::Error>> {
     
        if let Some(cache) = &self.cache_service {
            match cache.get_cached_comments(post_id) {
                Ok(Some(comments)) => {
                    info!("⚡ Retrieve {} comments from Redis cache", comments.len());
                    return Ok(comments);
                },
                Ok(None) => {
                    info!("📭 Comment cache miss, query database");
                },
                Err(e) => {
                    info!("⚠️ Redis cache query failed: {}", e);
                }
            }
        }

        if let Some(db) = &self.database_service {
            match db.get_comments_by_post_id(post_id).await {
                Ok(comments) => {
                    info!("📊 Retrieved {} comments from database", comments.len());
                    
                   
                    if let Some(cache) = &self.cache_service {
                        if let Err(e) = cache.cache_comments(post_id, &comments) {
                            info!("⚠️ Failed to cache comments: {}", e);
                        } else {
                            info!("💾 Comments cached to Redis");
                        }
                    }
                    
                    return Ok(comments);
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in-memory data: {}", e);
                }
            }
        }
        
        // Fallback to memory data
        let comments = self.comments.lock().unwrap();
        let post_comments: Vec<Comment> = comments
            .values()
            .filter(|comment| comment.post_id == post_id)
            .cloned()
            .collect();
        info!("📊 Retrieve {} comments from memory", post_comments.len());
        Ok(post_comments)
    }

    pub async fn get_user_profile(&self, address: &str) -> Option<User> {
        
        if let Some(db) = &self.database_service {
            match db.get_user_by_address(address).await {
                Ok(Some(user)) => {
                    info!("📊 Retrieved user profile from database: {} (posts: {}, comments: {}, reputation: {})", address, user.posts_count, user.comments_count, user.reputation);
                    return Some(user);
                },
                Ok(None) => {
                    info!("📊 User not found in database: {}", address);
                },
                Err(e) => {
                    info!("⚠️ Database query failed: {}", e);
                }
            }
        }
        
        
        let users = self.users.lock().unwrap();
        let posts = self.posts.lock().unwrap();
        let comments = self.comments.lock().unwrap();
        
        
        let actual_post_count = posts.values().filter(|post| post.author_address == address).count();
        let actual_comment_count = comments.values().filter(|comment| comment.author_address == address).count();
        
        info!("📊 In-memory stats for user {} - posts: {}, comments: {}", address, actual_post_count, actual_comment_count);
        
        
        if let Some(mut user) = users.get(address).cloned() {
            
            user.posts_count = actual_post_count as u32;
            user.comments_count = actual_comment_count as u32;
            user.reputation = (actual_post_count * 10 + actual_comment_count * 5) as u32;
            Some(user)
        } else if actual_post_count > 0 || actual_comment_count > 0 {
            
            Some(User {
                id: "temp".to_string(),
                address: address.to_string(),
                name: None,
                avatar: None,
                bio: None,
                created_at: Utc::now(),
                posts_count: actual_post_count as u32,
                comments_count: actual_comment_count as u32,
                reputation: (actual_post_count * 10 + actual_comment_count * 5) as u32,
            })
        } else {
            None
        }
    }

    async fn update_user_stats(&self, address: &str, is_post: bool, is_comment: bool) {
        let mut users = self.users.lock().unwrap();
        
        let user = users.entry(address.to_string()).or_insert_with(|| User {
            id: "temp".to_string(),
            address: address.to_string(),
            name: None,
            avatar: None,
            bio: None,
            created_at: Utc::now(),
            posts_count: 0,
            comments_count: 0,
            reputation: 0,
        });

        if is_post {
            user.posts_count += 1;
            user.reputation += 10;
        }
        
        if is_comment {
            user.comments_count += 1;
            user.reputation += 5;
        }
    }

    pub async fn upload_to_irys(&self, request: IrysUploadRequest) -> Result<String, Box<dyn std::error::Error>> {
        self.irys_service.upload_data(&request.data, request.tags, &request.address).await
    }

    // Get active users ranking
    pub async fn get_active_users_ranking(&self, limit: u32) -> Vec<User> {
        
        if let Some(db) = &self.database_service {
            match db.get_active_users_ranking(limit as i64).await {
                Ok(mut users) => {
                    info!("📊 Retrieved {} active users from database", users.len());

                    // Try to complete real username: if name is empty or default alias user_XXXX, query and sync username
                    for user in &mut users {
                        let needs_lookup = match &user.name {
                            None => true,
                            Some(n) => n.is_empty() || n.starts_with("user_"),
                        };
                        if needs_lookup {
                            if let Ok(Some(username)) = self.get_username_by_address(&user.address).await {
                                user.name = Some(username);
                            }
                        }
                    }

                    return users;
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in-memory data: {}", e);
                }
            }
        }
        
        // Fallback to memory data
        let users = self.users.lock().unwrap();
        let mut user_list: Vec<User> = users.values().cloned().collect();
        
        // Sort by reputation
        user_list.sort_by(|a, b| {
            b.reputation.cmp(&a.reputation)
                .then(b.posts_count.cmp(&a.posts_count))
                .then(b.comments_count.cmp(&a.comments_count))
        });
        
        // Only return users with activity
        let mut user_list: Vec<User> = user_list
            .into_iter()
            .filter(|user| user.posts_count > 0 || user.comments_count > 0)
            .take(limit as usize)
            .collect();

        // Sync and complete username
        for user in &mut user_list {
            let needs_lookup = match &user.name {
                None => true,
                Some(n) => n.is_empty() || n.starts_with("user_"),
            };
            if needs_lookup {
                if let Ok(Some(username)) = self.get_username_by_address(&user.address).await {
                    user.name = Some(username);
                }
            }
        }

        user_list
    }

    // Get global stats
    pub async fn get_global_stats(&self) -> GlobalStats {
        
        if let Some(db) = &self.database_service {
            match db.get_global_stats().await {
                Ok(stats) => {
                    info!("📊 Retrieved global stats from database: users={}, posts={}, comments={}, likes={}", stats.total_users, stats.total_posts, stats.total_comments, stats.total_likes);
                    return stats;
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in-memory data: {}", e);
                }
            }
        }
        
        // Fallback to memory data
        let users = self.users.lock().unwrap();
        let posts = self.posts.lock().unwrap();
        let comments = self.comments.lock().unwrap();
        
        let active_users = users.values().filter(|u| u.posts_count > 0 || u.comments_count > 0).count();
        let total_likes = posts.values().map(|p| p.likes).sum();
        
        GlobalStats {
            total_users: active_users as u32,
            total_posts: posts.len() as u32,
            total_comments: comments.len() as u32,
            total_likes,
        }
    }

    
    pub async fn like_post(&self, post_id: &str, user_address: &str) -> Result<u32, Box<dyn std::error::Error>> {
        
        if let Some(db) = &self.database_service {
            match db.like_post(post_id, user_address).await {
                Ok(new_likes) => {
                    info!("📊 Database like succeeded: post {} new likes {}", post_id, new_likes);
                    return Ok(new_likes);
                },
                Err(e) => {
                    info!("⚠️ Database like failed, using in-memory storage: {}", e);
                }
            }
        }
        
        
        let mut posts = self.posts.lock().unwrap();
        if let Some(post) = posts.get_mut(post_id) {
            post.likes += 1;
            Ok(post.likes)
        } else {
            Err("Post not found".into())
        }
    }

    pub async fn query_irys(&self, address: Option<String>, tags: Option<Vec<String>>, limit: Option<u32>) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        self.irys_service.query_data(address.as_deref(), tags, limit).await
    }
    
    // Register username
    pub async fn register_username(&self, address: &str, username: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // First check on-chain status
        if let Some(ref blockchain) = self.blockchain_service {
            match blockchain.user_has_username_on_chain(address).await {
                Ok(true) => {
                    info!("⚠️ User already has a username on-chain: {}", address);
                    // If username exists on-chain, try to sync to database
                    if let Some(ref db) = self.database_service {
                        if let Ok(Some(chain_username)) = blockchain.get_username_by_address_on_chain(address).await {
                            info!("📊 Sync on-chain username to database: {} -> {}", address, chain_username);
                            // Ensure user exists in database
                            db.ensure_user_exists(address, &None).await?;
                            // Update username in database
                            let _ = db.register_username(address, &chain_username).await;
                            // Return success because username already exists and is synced
                            return Ok(true);
                        }
                    }
                    return Ok(false);
                }
                Ok(false) => {
                    info!("📊 No on-chain username for user, can register: {}", address);
                }
                Err(e) => {
                    info!("⚠️ Failed to check on-chain username status: {}", e);
                }
            }
        }

        // Then register in database
        if let Some(ref db) = self.database_service {
            match db.register_username(address, username).await {
                Ok(success) => Ok(success),
                Err(e) => {
                    info!("⚠️ Database username registration failed: {}", e);
                    Err(e.into())
                }
            }
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // Check if username is available
    pub async fn is_username_available(&self, username: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            match db.is_username_available(username).await {
                Ok(available) => Ok(available),
                Err(e) => {
                    info!("⚠️ Database check username failed: {}", e);
                    Err(e.into())
                }
            }
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // Get username by address
    pub async fn get_username_by_address(&self, address: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // First get from database
        if let Some(ref db) = self.database_service {
            match db.get_username_by_address(address).await {
                Ok(Some(username)) => Ok(Some(username)),
                Ok(None) => {
                    // No username in database, try to get from chain
                    if let Some(ref blockchain) = self.blockchain_service {
                        match blockchain.get_username_by_address_on_chain(address).await {
                            Ok(Some(chain_username)) => {
                                info!("📊 Fetched username from chain and synced to database: {} -> {}", address, chain_username);
                                // Sync to database
                                self.sync_username_from_chain(address).await?;
                                Ok(Some(chain_username))
                            }
                            Ok(None) => Ok(None),
                            Err(e) => {
                                info!("⚠️ Failed to fetch username from chain: {}", e);
                                Ok(None)
                            }
                        }
                    } else {
                        Ok(None)
                    }
                },
                Err(e) => {
                    info!("⚠️ Failed to get username from database: {}", e);
                    Ok(None)
                }
            }
        } else {
            // Database unavailable, try to get from chain
            if let Some(ref blockchain) = self.blockchain_service {
                match blockchain.get_username_by_address_on_chain(address).await {
                    Ok(username) => Ok(username),
                    Err(e) => {
                        info!("⚠️ Failed to fetch username from chain: {}", e);
                        Ok(None)
                    }
                }
            } else {
                Ok(None)
            }
        }
    }
    
    // Check if user has registered username
    pub async fn user_has_username(&self, address: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // First check database
        if let Some(ref db) = self.database_service {
            match db.user_has_username(address).await {
                Ok(has_username) => {
                    if has_username {
                        return Ok(true);
                    }
                },
                Err(e) => {
                    info!("⚠️ Database check username status failed: {}", e);
                }
            }
        }

        // If database has no username, check on-chain status
        if let Some(ref blockchain) = self.blockchain_service {
            match blockchain.user_has_username_on_chain(address).await {
                Ok(true) => {
                    info!("📊 Found username on chain, sync to database: {}", address);
                    // Sync username from chain to database
                    self.sync_username_from_chain(address).await?;
                    Ok(true)
                }
                Ok(false) => Ok(false),
                Err(e) => {
                    info!("⚠️ Failed to check on-chain username status: {}", e);
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }

    // Sync username from chain to database
    async fn sync_username_from_chain(&self, address: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref blockchain) = self.blockchain_service {
            if let Some(ref db) = self.database_service {
                if let Ok(Some(chain_username)) = blockchain.get_username_by_address_on_chain(address).await {
                    info!("📊 Sync on-chain username to database: {} -> {}", address, chain_username);
                    // Ensure user exists in database
                    db.ensure_user_exists(address, &None).await?;
                    // Update username in database
                    let _ = db.register_username(address, &chain_username).await;
                }
            }
        }
        Ok(())
    }

    // Check if transaction is used
    pub async fn is_transaction_used(&self, tx_hash: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(database_service) = &self.database_service {
            database_service.is_transaction_used(tx_hash).await
                .map_err(|e| e.into())
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // Verify blockchain post transaction
    pub async fn verify_blockchain_post_transaction(
        &self, 
        tx_hash: &str, 
        expected_sender: &str
    ) -> Result<crate::blockchain::PostTransactionVerification, Box<dyn std::error::Error>> {
        if let Some(blockchain_service) = &self.blockchain_service {
            blockchain_service.verify_post_transaction(tx_hash, expected_sender).await
        } else {
            Err("Blockchain service unavailable".into())
        }
    }
    
    // Verify blockchain comment transaction
    pub async fn verify_blockchain_comment_transaction(
        &self, 
        tx_hash: &str, 
        expected_sender: &str
    ) -> Result<crate::blockchain::CommentTransactionVerification, Box<dyn std::error::Error>> {
        if let Some(blockchain_service) = &self.blockchain_service {
            blockchain_service.verify_comment_transaction(tx_hash, expected_sender).await
        } else {
            Err("Blockchain service unavailable".into())
        }
    }
    
    // Create post with blockchain verification
    pub async fn create_post_with_verification(
        &self, 
        request: CreatePostRequest,
        verification: crate::blockchain::PostTransactionVerification
    ) -> Result<Post, Box<dyn std::error::Error>> {
    
        if let Some(database_service) = &self.database_service {
            match database_service.check_duplicate_post(&request.author_address, &request.content).await {
                Ok(true) => {
                    return Err("You have posted the same content within the last 5 minutes. Please avoid duplicate posts.".into());
                }
                Ok(false) => {
                    info!("✅ Post content deduplication check passed");
                }
                Err(e) => {
                    info!("⚠️ Post content deduplication check failed, continuing: {}", e);
                }
            }
        }

        let post_data = serde_json::to_string(&request)?;
        let tags = vec!["forum".to_string(), "post".to_string()];
        let author_address = request.author_address.clone();
        
        // Get user's username (if any)
        let author_name = if let Ok(Some(username)) = self.get_username_by_address(&author_address).await {
            Some(username)
        } else {
            request.author_name
        };
        
        // Upload to Irys
        let tx_id = self.irys_service.upload_data(&post_data, tags, &author_address).await?;
        
        let post = Post {
            id: Self::generate_id(),
            title: request.title,
            content: request.content,
            author_address: request.author_address.clone(),
            author_id: None, 
            author_name,
            author_avatar: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            likes: 0,
            comments_count: 0,
            tags: request.tags,
            irys_transaction_id: Some(tx_id),
            image: request.image,
            blockchain_post_id: request.blockchain_post_id,
            is_liked_by_user: false, 
            views: 0, // New post views count is 0
            heat_score: None, // Heat score will be calculated later
        };
        
                      
         if let Some(database_service) = &self.database_service {
            
             database_service.create_post(&post).await?;
             
            
             if let Some(tx_hash) = &request.blockchain_transaction_hash {
                 database_service.update_post_blockchain_hash(&post.id, tx_hash).await?;
                 
              
                 let block_timestamp = chrono::DateTime::from_timestamp(
                     verification.block_timestamp.as_u64() as i64, 0
                 ).unwrap_or_else(|| Utc::now());
                 
                 database_service.record_post_transaction(
                     tx_hash,
                     &verification.sender,
                     verification.block_number,
                     block_timestamp,
                     &post.id
                 ).await?;
             }
             
             
             if let Some(cache) = &self.cache_service {
                 if let Err(e) = cache.invalidate_post_cache() {
                     info!("⚠️ Failed to clear post cache: {}", e);
                 } else {
                     info!("🗑️ Cleared post list cache (verified creation)");
                 }
             }
         } else {
             return Err("Database service unavailable".into());
         }
        
        info!("✅ Post created successfully, blockchain transaction verified: {}", verification.transaction_hash);
        Ok(post)
    }
    
    
    pub async fn add_comment_with_verification(
        &self,
        request: CreateCommentRequest,
        verification: crate::blockchain::CommentTransactionVerification
    ) -> Result<Comment, Box<dyn std::error::Error>> {
      
        if let Some(database_service) = &self.database_service {
            match database_service.check_duplicate_comment(&request.author_address, &request.content, &request.post_id).await {
                Ok(true) => {
                    return Err("You have posted the same comment within the last 5 minutes. Please avoid duplicate comments.".into());
                }
                Ok(false) => {
                    info!("✅ Comment content deduplication check passed");
                }
                Err(e) => {
                    info!("⚠️ Comment content deduplication check failed, continuing: {}", e);
                }
            }
        }

        let comment_data = serde_json::to_string(&request)?;
        let tags = vec!["forum".to_string(), "comment".to_string()];
        let author_address = request.author_address.clone();
        
     
        let author_name = if let Ok(Some(username)) = self.get_username_by_address(&author_address).await {
            Some(username)
        } else {
            request.author_name
        };
        
 
        let tx_id = self.irys_service.upload_data(&comment_data, tags, &author_address).await?;
        
        let comment = Comment {
            id: Self::generate_id(),
            post_id: request.post_id.clone(),
            content: request.content.clone(),
            author_address: request.author_address.clone(),
            author_id: None, 
            author_name: author_name.clone(),
            author_avatar: None,
            created_at: Utc::now(),
            parent_id: request.parent_id.clone(),
            likes: 0,
            irys_transaction_id: Some(tx_id),
            image: request.image.clone(),
            content_hash: Self::generate_content_hash(&request.content),
            is_liked_by_user: false,
        };
        
               
         if let Some(database_service) = &self.database_service {
        
             database_service.add_comment(&comment).await?;
             
          
             if let Some(tx_hash) = &request.blockchain_transaction_hash {
                 database_service.update_comment_blockchain_hash(&comment.id, tx_hash).await?;
                 
             
                 let block_timestamp = chrono::DateTime::from_timestamp(
                     verification.block_timestamp.as_u64() as i64, 0
                 ).unwrap_or_else(|| Utc::now());
                 
                 database_service.record_comment_transaction(
                     tx_hash,
                     &verification.sender,
                     verification.block_number,
                     block_timestamp,
                     &comment.id
                 ).await?;
             }
             
         
             if let Some(cache) = &self.cache_service {
                 if let Err(e) = cache.invalidate_comment_cache(&comment.post_id) {
                     info!("⚠️ Failed to clear comment cache: {}", e);
                 } else {
                     info!("🗑️ Cleared post comment cache (verified creation)");
                 }
             }
         } else {
             return Err("Database service unavailable".into());
         }
        
        info!("✅ Comment created successfully, blockchain transaction verified: {}", verification.transaction_hash);
        Ok(comment)
    }
    
    pub fn get_database_performance(&self) -> Option<serde_json::Value> {
        if let Some(database_service) = &self.database_service {
            Some(database_service.get_database_stats())
        } else {
            None
        }
    }
    
  
    pub fn has_cache_service(&self) -> bool {
        self.cache_service.is_some()
    }
    
 
    pub fn get_memory_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "posts_in_memory": self.posts.lock().unwrap().len(),
            "comments_in_memory": self.comments.lock().unwrap().len(),
            "users_in_memory": self.users.lock().unwrap().len()
        })
    }
    
    
    pub async fn create_post_async(&self, request: CreatePostRequest) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(async_queue) = &self.async_queue_service {
            if let Some(tx_hash) = request.blockchain_transaction_hash.clone() {
                // Submit to async queue
                let task_id = async_queue.submit_post_creation(request, tx_hash).await?;
                info!("🚀 Post creation task submitted to async queue: {}", task_id);
                Ok(task_id)
            } else {
                Err("Missing blockchain transaction hash".into())
            }
        } else {
            // Fallback to sync processing
            self.create_post_with_verification(
                request,
                crate::blockchain::PostTransactionVerification {
                    transaction_hash: "sync".to_string(),
                    sender: "unknown".to_string(),
                    block_number: 0,
                    block_timestamp: ethers::types::U256::zero(),
                    post_id: ethers::types::U256::zero(),
                    points_earned: ethers::types::U256::zero(),
                    value_paid: ethers::types::U256::zero(),
                    gas_used: ethers::types::U256::zero(),
                    verified: true,
                }
            ).await.map(|post| post.id)
        }
    }
    
    // Asynchronous create comment - immediately return task ID
    pub async fn create_comment_async(&self, request: CreateCommentRequest) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(async_queue) = &self.async_queue_service {
            if let Some(tx_hash) = request.blockchain_transaction_hash.clone() {
                // Submit to async queue
                let task_id = async_queue.submit_comment_creation(request, tx_hash).await?;
                info!("🚀 Comment creation task submitted to async queue: {}", task_id);
                Ok(task_id)
            } else {
                Err("Missing blockchain transaction hash".into())
            }
        } else {
            // Fallback to sync processing
            self.add_comment_with_verification(
                request,
                crate::blockchain::CommentTransactionVerification {
                    transaction_hash: "sync".to_string(),
                    sender: "unknown".to_string(),
                    block_number: 0,
                    block_timestamp: ethers::types::U256::zero(),
                    comment_id: ethers::types::U256::zero(),
                    post_id: ethers::types::U256::zero(),
                    points_earned: ethers::types::U256::zero(),
                    value_paid: ethers::types::U256::zero(),
                    gas_used: ethers::types::U256::zero(),
                    verified: true,
                }
            ).await.map(|comment| comment.id)
        }
    }
    
    // Query async task status
    pub async fn get_task_status(&self, task_id: &str) -> Option<serde_json::Value> {
        if let Some(async_queue) = &self.async_queue_service {
            if let Some(result) = async_queue.get_task_status(task_id).await {
                Some(serde_json::json!({
                    "task_id": task_id,
                    "status": format!("{:?}", result.status),
                    "result": result.result_data,
                    "created_at": result.created_at,
                    "completed_at": result.completed_at
                }))
            } else {
                None
            }
        } else {
            None
        }
    }
    
    // Like comment
    pub async fn like_comment(&self, comment_id: &str, user_address: &str) -> Result<(u32, bool), Box<dyn std::error::Error + Send + Sync>> {
        // Call database service to update like count
        if let Some(db) = &self.database_service {
            db.like_comment(comment_id, user_address).await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // Get comments with like status
    pub async fn get_comments_with_like_status(&self, post_id: &str, user_address: Option<&str>) -> Result<Vec<Comment>, Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            match db.get_comments_by_post_id(post_id).await {
                Ok(mut comments) => {
                    // If user address is provided, check like status for each comment
                    if let Some(user_addr) = user_address {
                        for comment in &mut comments {
                            if let Ok(is_liked) = db.check_comment_liked(&comment.id, user_addr).await {
                                comment.is_liked_by_user = is_liked;
                            }
                        }
                    }
                    Ok(comments)
                }
                Err(e) => Err(Box::new(e))
            }
        } else {
            
            let comments_map = self.comments.lock().unwrap();
            let comments: Vec<Comment> = comments_map.values()
                .filter(|comment| comment.post_id == post_id)
                .cloned()
                .collect();
            Ok(comments)
        }
    }

    // Get comments with like status (paginated version)
    pub async fn get_comments_with_like_status_paginated(&self, post_id: &str, user_address: Option<&str>, limit: u32, offset: u32) -> Result<Vec<Comment>, Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            match db.get_comments_by_post_id_paginated(post_id, limit, offset).await {
                Ok(mut comments) => {
                    // If user address is provided, check like status for each comment
                    if let Some(user_addr) = user_address {
                        for comment in &mut comments {
                            if let Ok(is_liked) = db.check_comment_liked(&comment.id, user_addr).await {
                                comment.is_liked_by_user = is_liked;
                            }
                        }
                    }
                    Ok(comments)
                }
                Err(e) => Err(Box::new(e))
            }
        } else {
            // Simple pagination
            let comments_map = self.comments.lock().unwrap();
            let mut comments: Vec<Comment> = comments_map.values()
                .filter(|comment| comment.post_id == post_id)
                .cloned()
                .collect();
            
            // Sort by time
            comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            
            // Apply pagination
            let start = offset as usize;
            let end = (start + limit as usize).min(comments.len());
            let paginated_comments = if start < comments.len() {
                comments[start..end].to_vec()
            } else {
                Vec::new()
            };
            
            Ok(paginated_comments)
        }
    }
    
    // Get user's own posts
    pub async fn get_user_posts(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
        // 1. First get from database
        if let Some(db) = &self.database_service {
            match db.get_posts_by_user(user_address, limit, offset).await {
                Ok(posts) => {
                    info!("📊 Get user posts from database: {} (count: {})", user_address, posts.len());
                    return Ok(posts);
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in-memory data: {}", e);
                }
            }
        }
        
        // 2. Fallback to memory data
        let posts = self.posts.lock().unwrap();
        let mut user_posts: Vec<Post> = posts
            .values()
            .filter(|post| post.author_address.eq_ignore_ascii_case(user_address))
            .cloned()
            .collect();
        
            // Sort by creation time (latest first)
        user_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // Apply pagination
        let start = offset as usize;
        let end = (start + limit as usize).min(user_posts.len());
        let paginated_posts = if start < user_posts.len() {
            user_posts[start..end].to_vec()
        } else {
            Vec::new()
        };
        
        info!("📊 Get user posts from memory: {} (count: {})", user_address, paginated_posts.len());
        Ok(paginated_posts)
    }

    // Get user posts (with like status)
    pub async fn get_user_posts_with_like_status(&self, user_address: &str, limit: u32, offset: u32, request_user_address: Option<&str>) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
        // 1. First get from database
        if let Some(db) = &self.database_service {
            match db.get_posts_by_user_with_like_status(user_address, limit, offset, request_user_address).await {
                Ok(posts) => {
                    info!("📊 Get user posts from database (with like status): {} (count: {})", user_address, posts.len());
                    return Ok(posts);
                },
                Err(e) => {
                    info!("⚠️ Database query failed, fallback to no like status: {}", e);
                    // Fallback to query without like status
                    return self.get_user_posts(user_address, limit, offset).await;
                }
            }
        }
        
        // 2. Fallback to memory data (without like status)
        self.get_user_posts(user_address, limit, offset).await
    }

    // Get user address by ID
    pub async fn get_user_address_by_id(&self, user_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            match db.get_user_address_by_id(user_id).await {
                Ok(address) => Ok(address),
                Err(sqlx::Error::RowNotFound) => Err(format!("User ID {} does not exist", user_id).into()),
                Err(e) => Err(e.into())
            }
        } else {
            Err("Database service unavailable".into())
        }
    }

    // Update user avatar
    pub async fn update_user_avatar(&self, user_address: &str, avatar_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            db.update_user_avatar(user_address, avatar_url).await?;
        }
        Ok(())
    }

    // Update user bio
    pub async fn update_user_bio(&self, user_address: &str, bio: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            db.update_user_bio(user_address, bio).await?;
        }
        Ok(())
    }

    // Get daily recommendations
    pub async fn get_daily_recommendations(&self, user_address: Option<&str>) -> Result<RecommendationResult, Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            // Check if need to refresh recommendations
            let should_refresh = db.should_refresh_daily_recommendations().await?;
            
            if should_refresh {
                info!("🔄 Start calculating today's hot posts...");
                
                // Calculate hot posts
                let hot_posts = db.calculate_hot_posts().await?;
                
                    // Update cache
                db.update_daily_recommendations(&hot_posts).await?;
                
                info!("✅ Today's recommendations updated, {} hot posts", hot_posts.len());
            }
            
            // Get recommendation result
            let result = db.get_daily_recommendations(user_address).await?;
            Ok(result)
        } else {
            // Fallback to memory mode, return empty result
            Ok(RecommendationResult {
                posts: vec![],
                last_refresh_time: None,
            })
        }
    }

    // Follow system related methods
    pub async fn follow_user(&self, request: FollowRequest) -> Result<FollowResponse, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let (follower_addr, following_addr) = if let (Some(follower_addr), Some(following_addr)) = 
                (request.follower_address.as_deref(), request.following_address.as_deref()) {
                // Based on address (backward compatibility)
                (follower_addr.to_string(), following_addr.to_string())
            } else if let (Some(follower_id), Some(following_id)) = 
                (request.follower_id.as_deref(), request.following_id.as_deref()) {
                // Based on ID, need to query address
                let follower_addr = self.get_user_address_by_id(follower_id).await?;
                let following_addr = self.get_user_address_by_id(following_id).await?;
                (follower_addr, following_addr)
            } else {
                return Err("Need to provide follower_address and following_address, or follower_id and following_id".into());
            };
                
            let success = db.follow_user(&follower_addr, &following_addr).await?;
            
            if success {
                // Get updated follow data
                let (following_count, followers_count, _) = db.get_follow_counts(&following_addr).await.unwrap_or((0, 0, 0));
                
                info!("👥 User follow success: {} followed {}", follower_addr, following_addr);
                
                Ok(FollowResponse {
                    success: true,
                    is_following: true,
                    following_count,
                    followers_count,
                })
            } else {
                info!("⚠️ User already followed: {} -> {}", follower_addr, following_addr);
                
                // Get current follow data
                let (following_count, followers_count, _) = db.get_follow_counts(&following_addr).await.unwrap_or((0, 0, 0));
                
                Ok(FollowResponse {
                    success: false,
                    is_following: true,
                    following_count,
                    followers_count,
                })
            }
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn unfollow_user(&self, request: FollowRequest) -> Result<FollowResponse, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let (follower_addr, following_addr) = if let (Some(follower_addr), Some(following_addr)) = 
                (request.follower_address.as_deref(), request.following_address.as_deref()) {
                // Based on address (backward compatibility)
                (follower_addr.to_string(), following_addr.to_string())
            } else if let (Some(follower_id), Some(following_id)) = 
                (request.follower_id.as_deref(), request.following_id.as_deref()) {
                // Based on ID, need to query address
                let follower_addr = self.get_user_address_by_id(follower_id).await?;
                let following_addr = self.get_user_address_by_id(following_id).await?;
                (follower_addr, following_addr)
            } else {
                return Err("Need to provide follower_address and following_address, or follower_id and following_id".into());
            };
                
            let success = db.unfollow_user(&follower_addr, &following_addr).await?;
            
                    // Get updated follow data
            let (following_count, followers_count, _) = db.get_follow_counts(&following_addr).await.unwrap_or((0, 0, 0));
            
            if success {
                info!("👥 User unfollow success: {} unfollowed {}", follower_addr, following_addr);
            } else {
                info!("⚠️ User not followed: {} -> {}", follower_addr, following_addr);
            }
            
            Ok(FollowResponse {
                success,
                is_following: false,
                following_count,
                followers_count,
            })
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn get_following_list(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<UserProfile>, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let profiles = db.get_following_list(user_address, limit as i64, offset as i64).await?;
            info!("📋 Get following list: {} (count: {})", user_address, profiles.len());
            Ok(profiles)
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn get_followers_list(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<UserProfile>, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let profiles = db.get_followers_list(user_address, limit as i64, offset as i64).await?;
            info!("📋 Get followers list: {} (count: {})", user_address, profiles.len());
            Ok(profiles)
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn get_mutual_follows_list(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<UserProfile>, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let profiles = db.get_mutual_follows_list(user_address, limit as i64, offset as i64).await?;
            info!("📋 Get mutual follows list: {} (count: {})", user_address, profiles.len());
            Ok(profiles)
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn get_follow_counts(&self, user_address: &str) -> Result<(u32, u32, u32), Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let counts = db.get_follow_counts(user_address).await?;
            Ok(counts)
        } else {
            Ok((0, 0, 0))
        }
    }

    pub async fn is_following(&self, follower_address: &str, following_address: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let is_following = db.is_following(follower_address, following_address).await?;
            Ok(is_following)
        } else {
            Ok(false)
        }
    }
} 
