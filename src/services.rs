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
                    
                    // 3. 将结果缓存到Redis
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
        
        // 回退到内存数据
        let comments = self.comments.lock().unwrap();
        let post_comments: Vec<Comment> = comments
            .values()
            .filter(|comment| comment.post_id == post_id)
            .cloned()
            .collect();
        info!("📊 从内存获取到 {} 个评论", post_comments.len());
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

    // 获取活跃用户排行榜
    pub async fn get_active_users_ranking(&self, limit: u32) -> Vec<User> {
        
        if let Some(db) = &self.database_service {
            match db.get_active_users_ranking(limit as i64).await {
                Ok(mut users) => {
                    info!("📊 从数据库获取到 {} 个活跃用户", users.len());

                    // 尝试补全真实用户名：如果 name 为空或是默认别名 user_XXXX，则查询并同步用户名
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
        
        // 回退到内存数据
        let users = self.users.lock().unwrap();
        let mut user_list: Vec<User> = users.values().cloned().collect();
        
        // 按声望排序
        user_list.sort_by(|a, b| {
            b.reputation.cmp(&a.reputation)
                .then(b.posts_count.cmp(&a.posts_count))
                .then(b.comments_count.cmp(&a.comments_count))
        });
        
        // 只返回有活动的用户
        let mut user_list: Vec<User> = user_list
            .into_iter()
            .filter(|user| user.posts_count > 0 || user.comments_count > 0)
            .take(limit as usize)
            .collect();

        // 同步并补全用户名
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

    // 获取全局统计数据
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
        
        // 回退到内存数据
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
    
    // 注册用户名
    pub async fn register_username(&self, address: &str, username: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // 首先检查链上状态
        if let Some(ref blockchain) = self.blockchain_service {
            match blockchain.user_has_username_on_chain(address).await {
                Ok(true) => {
                    info!("⚠️ User already has a username on-chain: {}", address);
                    // 如果链上已有用户名，尝试同步到数据库
                    if let Some(ref db) = self.database_service {
                        if let Ok(Some(chain_username)) = blockchain.get_username_by_address_on_chain(address).await {
                            info!("📊 Sync on-chain username to database: {} -> {}", address, chain_username);
                            // 确保用户存在于数据库中
                            db.ensure_user_exists(address, &None).await?;
                            // 更新数据库中的用户名
                            let _ = db.register_username(address, &chain_username).await;
                            // 返回成功，因为用户名已经存在且已同步
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

        // 然后进行数据库注册
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
    
    // 检查用户名是否可用
    pub async fn is_username_available(&self, username: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            match db.is_username_available(username).await {
                Ok(available) => Ok(available),
                Err(e) => {
                    info!("⚠️ 数据库检查用户名失败: {}", e);
                    Err(e.into())
                }
            }
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // 根据地址获取用户名
    pub async fn get_username_by_address(&self, address: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // 首先从数据库获取
        if let Some(ref db) = self.database_service {
            match db.get_username_by_address(address).await {
                Ok(Some(username)) => Ok(Some(username)),
                Ok(None) => {
                    // 数据库中没有，尝试从链上获取
                    if let Some(ref blockchain) = self.blockchain_service {
                        match blockchain.get_username_by_address_on_chain(address).await {
                            Ok(Some(chain_username)) => {
                                info!("📊 Fetched username from chain and synced to database: {} -> {}", address, chain_username);
                                // 同步到数据库
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
            // 数据库不可用，尝试从链上获取
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
    
    // 检查用户是否已注册用户名
    pub async fn user_has_username(&self, address: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // 首先检查数据库
        if let Some(ref db) = self.database_service {
            match db.user_has_username(address).await {
                Ok(has_username) => {
                    if has_username {
                        return Ok(true);
                    }
                },
                Err(e) => {
                    info!("⚠️ 数据库检查用户名状态失败: {}", e);
                }
            }
        }

        // 如果数据库中没有，检查链上状态
        if let Some(ref blockchain) = self.blockchain_service {
            match blockchain.user_has_username_on_chain(address).await {
                Ok(true) => {
                    info!("📊 链上发现用户名，同步到数据库: {}", address);
                    // 同步链上用户名到数据库
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

    // 同步链上用户名到数据库
    async fn sync_username_from_chain(&self, address: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref blockchain) = self.blockchain_service {
            if let Some(ref db) = self.database_service {
                if let Ok(Some(chain_username)) = blockchain.get_username_by_address_on_chain(address).await {
                    info!("📊 Sync on-chain username to database: {} -> {}", address, chain_username);
                    // 确保用户存在于数据库中
                    db.ensure_user_exists(address, &None).await?;
                    // 更新数据库中的用户名
                    let _ = db.register_username(address, &chain_username).await;
                }
            }
        }
        Ok(())
    }

    // 检查交易是否已被使用
    pub async fn is_transaction_used(&self, tx_hash: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(database_service) = &self.database_service {
            database_service.is_transaction_used(tx_hash).await
                .map_err(|e| e.into())
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // 验证区块链发帖交易
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
    
    // 验证区块链评论交易
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
    
    // 带区块链验证的创建帖子
    pub async fn create_post_with_verification(
        &self, 
        request: CreatePostRequest,
        verification: crate::blockchain::PostTransactionVerification
    ) -> Result<Post, Box<dyn std::error::Error>> {
        // 防重复内容检查：检查用户在最近5分钟内是否发布了相同内容的帖子
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
        
        // 获取用户的用户名（如果有的话）
        let author_name = if let Ok(Some(username)) = self.get_username_by_address(&author_address).await {
            Some(username)
        } else {
            request.author_name
        };
        
        // 上传到Irys
        let tx_id = self.irys_service.upload_data(&post_data, tags, &author_address).await?;
        
        let post = Post {
            id: Self::generate_id(),
            title: request.title,
            content: request.content,
            author_address: request.author_address.clone(),
            author_id: None, // 将在数据库存储时填充
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
            is_liked_by_user: false, // 新帖子默认未点赞
            views: 0, // 新帖子浏览量为0
            heat_score: None, // 热度分数稍后计算
        };
        
                          // 创建帖子并记录交易
         if let Some(database_service) = &self.database_service {
             // 先创建帖子
             database_service.create_post(&post).await?;
             
             // 然后更新区块链交易哈希并记录交易
             if let Some(tx_hash) = &request.blockchain_transaction_hash {
                 database_service.update_post_blockchain_hash(&post.id, tx_hash).await?;
                 
                 // 记录已使用的交易
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
             
             // 清除帖子列表缓存
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
    
    // 带区块链验证的创建评论
    pub async fn add_comment_with_verification(
        &self,
        request: CreateCommentRequest,
        verification: crate::blockchain::CommentTransactionVerification
    ) -> Result<Comment, Box<dyn std::error::Error>> {
        // 防重复内容检查：检查用户在最近5分钟内是否发布了相同内容的评论
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
        
        // 获取用户的用户名（如果有的话）
        let author_name = if let Ok(Some(username)) = self.get_username_by_address(&author_address).await {
            Some(username)
        } else {
            request.author_name
        };
        
        // 上传到Irys
        let tx_id = self.irys_service.upload_data(&comment_data, tags, &author_address).await?;
        
        let comment = Comment {
            id: Self::generate_id(),
            post_id: request.post_id.clone(),
            content: request.content.clone(),
            author_address: request.author_address.clone(),
            author_id: None, // 将在数据库层根据author_address获取
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
        
                 // 保存评论和交易记录
         if let Some(database_service) = &self.database_service {
             // 添加评论
             database_service.add_comment(&comment).await?;
             
             // 更新评论的区块链交易哈希
             if let Some(tx_hash) = &request.blockchain_transaction_hash {
                 database_service.update_comment_blockchain_hash(&comment.id, tx_hash).await?;
                 
                 // 记录已使用的交易
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
             
             // 清除相关评论缓存
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
    
    // 获取数据库性能统计
    pub fn get_database_performance(&self) -> Option<serde_json::Value> {
        if let Some(database_service) = &self.database_service {
            Some(database_service.get_database_stats())
        } else {
            None
        }
    }
    
    // 检查是否有缓存服务
    pub fn has_cache_service(&self) -> bool {
        self.cache_service.is_some()
    }
    
    // 获取内存统计
    pub fn get_memory_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "posts_in_memory": self.posts.lock().unwrap().len(),
            "comments_in_memory": self.comments.lock().unwrap().len(),
            "users_in_memory": self.users.lock().unwrap().len()
        })
    }
    
    // 异步创建帖子 - 立即返回任务ID
    pub async fn create_post_async(&self, request: CreatePostRequest) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(async_queue) = &self.async_queue_service {
            if let Some(tx_hash) = request.blockchain_transaction_hash.clone() {
                // 提交到异步队列
                let task_id = async_queue.submit_post_creation(request, tx_hash).await?;
                info!("🚀 帖子创建任务已提交到异步队列: {}", task_id);
                Ok(task_id)
            } else {
                Err("缺少区块链交易哈希".into())
            }
        } else {
            // 回退到同步处理
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
    
    // 异步创建评论 - 立即返回任务ID
    pub async fn create_comment_async(&self, request: CreateCommentRequest) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(async_queue) = &self.async_queue_service {
            if let Some(tx_hash) = request.blockchain_transaction_hash.clone() {
                // 提交到异步队列
                let task_id = async_queue.submit_comment_creation(request, tx_hash).await?;
                info!("🚀 评论创建任务已提交到异步队列: {}", task_id);
                Ok(task_id)
            } else {
                Err("缺少区块链交易哈希".into())
            }
        } else {
            // 回退到同步处理
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
    
    // 查询异步任务状态
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
    
    // 点赞评论
    pub async fn like_comment(&self, comment_id: &str, user_address: &str) -> Result<(u32, bool), Box<dyn std::error::Error + Send + Sync>> {
        // 调用数据库服务更新点赞数
        if let Some(db) = &self.database_service {
            db.like_comment(comment_id, user_address).await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            Err("Database service unavailable".into())
        }
    }
    
    // 获取带点赞状态的评论列表
    pub async fn get_comments_with_like_status(&self, post_id: &str, user_address: Option<&str>) -> Result<Vec<Comment>, Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            match db.get_comments_by_post_id(post_id).await {
                Ok(mut comments) => {
                    // 如果提供了用户地址，检查每个评论的点赞状态
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

    // 获取带点赞状态的评论列表（分页版本）
    pub async fn get_comments_with_like_status_paginated(&self, post_id: &str, user_address: Option<&str>, limit: u32, offset: u32) -> Result<Vec<Comment>, Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            match db.get_comments_by_post_id_paginated(post_id, limit, offset).await {
                Ok(mut comments) => {
                    // 如果提供了用户地址，检查每个评论的点赞状态
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
            （简单分页）
            let comments_map = self.comments.lock().unwrap();
            let mut comments: Vec<Comment> = comments_map.values()
                .filter(|comment| comment.post_id == post_id)
                .cloned()
                .collect();
            
            // 按时间排序
            comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            
            // 应用分页
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
    
    // 获取用户自己的帖子
    pub async fn get_user_posts(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
        // 1. 优先从数据库获取
        if let Some(db) = &self.database_service {
            match db.get_posts_by_user(user_address, limit, offset).await {
                Ok(posts) => {
                    info!("📊 从数据库获取用户帖子: {} (数量: {})", user_address, posts.len());
                    return Ok(posts);
                },
                Err(e) => {
                    info!("⚠️ Database query failed, using in-memory data: {}", e);
                }
            }
        }
        
        // 2. 回退到内存数据
        let posts = self.posts.lock().unwrap();
        let mut user_posts: Vec<Post> = posts
            .values()
            .filter(|post| post.author_address.eq_ignore_ascii_case(user_address))
            .cloned()
            .collect();
        
        // 按创建时间降序排序（最新的在前面）
        user_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // 应用分页
        let start = offset as usize;
        let end = (start + limit as usize).min(user_posts.len());
        let paginated_posts = if start < user_posts.len() {
            user_posts[start..end].to_vec()
        } else {
            Vec::new()
        };
        
        info!("📊 从内存获取用户帖子: {} (数量: {})", user_address, paginated_posts.len());
        Ok(paginated_posts)
    }

    // 获取用户帖子（包含点赞状态）
    pub async fn get_user_posts_with_like_status(&self, user_address: &str, limit: u32, offset: u32, request_user_address: Option<&str>) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
        // 1. 优先从数据库获取
        if let Some(db) = &self.database_service {
            match db.get_posts_by_user_with_like_status(user_address, limit, offset, request_user_address).await {
                Ok(posts) => {
                    info!("📊 从数据库获取用户帖子(含点赞状态): {} (数量: {})", user_address, posts.len());
                    return Ok(posts);
                },
                Err(e) => {
                    info!("⚠️ 数据库查询失败，回退到无点赞状态: {}", e);
                    // 回退到不含点赞状态的查询
                    return self.get_user_posts(user_address, limit, offset).await;
                }
            }
        }
        
        // 2. 回退到内存数据（不含点赞状态）
        self.get_user_posts(user_address, limit, offset).await
    }

    // 根据用户ID获取地址的公共方法
    pub async fn get_user_address_by_id(&self, user_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            match db.get_user_address_by_id(user_id).await {
                Ok(address) => Ok(address),
                Err(sqlx::Error::RowNotFound) => Err(format!("用户ID {} 不存在", user_id).into()),
                Err(e) => Err(e.into())
            }
        } else {
            Err("Database service unavailable".into())
        }
    }

    // 更新用户头像
    pub async fn update_user_avatar(&self, user_address: &str, avatar_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            db.update_user_avatar(user_address, avatar_url).await?;
        }
        Ok(())
    }

    // 更新用户个人简介
    pub async fn update_user_bio(&self, user_address: &str, bio: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            db.update_user_bio(user_address, bio).await?;
        }
        Ok(())
    }

    // 获取每日推荐
    pub async fn get_daily_recommendations(&self, user_address: Option<&str>) -> Result<RecommendationResult, Box<dyn std::error::Error>> {
        if let Some(db) = &self.database_service {
            // 检查是否需要刷新推荐
            let should_refresh = db.should_refresh_daily_recommendations().await?;
            
            if should_refresh {
                info!("🔄 开始计算今日热门帖子...");
                
                // 计算热门帖子
                let hot_posts = db.calculate_hot_posts().await?;
                
                // 更新缓存
                db.update_daily_recommendations(&hot_posts).await?;
                
                info!("✅ 今日推荐已更新，共 {} 个热门帖子", hot_posts.len());
            }
            
            // 获取推荐结果
            let result = db.get_daily_recommendations(user_address).await?;
            Ok(result)
        } else {
            // 回退到内存模式，返回空结果
            Ok(RecommendationResult {
                posts: vec![],
                last_refresh_time: None,
            })
        }
    }

    // 关注系统相关方法
    pub async fn follow_user(&self, request: FollowRequest) -> Result<FollowResponse, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let (follower_addr, following_addr) = if let (Some(follower_addr), Some(following_addr)) = 
                (request.follower_address.as_deref(), request.following_address.as_deref()) {
                // 基于地址的操作（向后兼容）
                (follower_addr.to_string(), following_addr.to_string())
            } else if let (Some(follower_id), Some(following_id)) = 
                (request.follower_id.as_deref(), request.following_id.as_deref()) {
                // 基于ID的操作，需要查询地址
                let follower_addr = self.get_user_address_by_id(follower_id).await?;
                let following_addr = self.get_user_address_by_id(following_id).await?;
                (follower_addr, following_addr)
            } else {
                return Err("需要提供 follower_address 和 following_address，或者 follower_id 和 following_id".into());
            };
                
            let success = db.follow_user(&follower_addr, &following_addr).await?;
            
            if success {
                // 获取更新后的关注数据
                let (following_count, followers_count, _) = db.get_follow_counts(&following_addr).await.unwrap_or((0, 0, 0));
                
                info!("👥 用户关注成功: {} 关注了 {}", follower_addr, following_addr);
                
                Ok(FollowResponse {
                    success: true,
                    is_following: true,
                    following_count,
                    followers_count,
                })
            } else {
                info!("⚠️ 用户已经关注: {} -> {}", follower_addr, following_addr);
                
                // 获取当前关注数据
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
                // 基于地址的操作（向后兼容）
                (follower_addr.to_string(), following_addr.to_string())
            } else if let (Some(follower_id), Some(following_id)) = 
                (request.follower_id.as_deref(), request.following_id.as_deref()) {
                // 基于ID的操作，需要查询地址
                let follower_addr = self.get_user_address_by_id(follower_id).await?;
                let following_addr = self.get_user_address_by_id(following_id).await?;
                (follower_addr, following_addr)
            } else {
                return Err("需要提供 follower_address 和 following_address，或者 follower_id 和 following_id".into());
            };
                
            let success = db.unfollow_user(&follower_addr, &following_addr).await?;
            
            // 获取更新后的关注数据
            let (following_count, followers_count, _) = db.get_follow_counts(&following_addr).await.unwrap_or((0, 0, 0));
            
            if success {
                info!("👥 用户取消关注成功: {} 取消关注了 {}", follower_addr, following_addr);
            } else {
                info!("⚠️ 用户未关注: {} -> {}", follower_addr, following_addr);
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
            info!("📋 获取关注列表: {} (数量: {})", user_address, profiles.len());
            Ok(profiles)
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn get_followers_list(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<UserProfile>, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let profiles = db.get_followers_list(user_address, limit as i64, offset as i64).await?;
            info!("📋 获取粉丝列表: {} (数量: {})", user_address, profiles.len());
            Ok(profiles)
        } else {
            Err("Database service unavailable".into())
        }
    }

    pub async fn get_mutual_follows_list(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<UserProfile>, Box<dyn std::error::Error>> {
        if let Some(ref db) = self.database_service {
            let profiles = db.get_mutual_follows_list(user_address, limit as i64, offset as i64).await?;
            info!("📋 获取朋友列表: {} (数量: {})", user_address, profiles.len());
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