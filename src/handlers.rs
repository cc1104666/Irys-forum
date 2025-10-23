use crate::models::*;
use crate::services::ForumService;
use actix_web::{web, HttpResponse, Result, Responder};
use log::{error, info};
use serde_json::{Value, json};
use std::sync::Arc;
use std::collections::HashMap;
use actix_multipart::Multipart;
use futures::TryStreamExt;
use std::io::Write;
use uuid::Uuid;
use serde::Deserialize;

pub async fn get_posts(
    service: web::Data<Arc<ForumService>>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    
    let limit = query.get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(15); 
    
    let offset = query.get("offset")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0); 
    
    
    let user_address = query.get("user_address").map(|s| s.as_str());
    
    let posts = service.get_posts_paginated_with_like_status(limit, offset, user_address).await;
    info!("Retrieved {} posts (limit: {}, offset: {}, user: {:?})", posts.len(), limit, offset, user_address);
    Ok(HttpResponse::Ok().json(ApiResponse::success(posts)))
}

pub async fn create_post(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<CreatePostRequest>,
) -> Result<HttpResponse> {
    info!("Creating new post: {}", request.title);
    
    let request_data = request.into_inner();
    
    
    if let Some(tx_hash) = &request_data.blockchain_transaction_hash {
        info!("Verifying smart contract transaction: {}", tx_hash);
        
       
        if !tx_hash.starts_with("0x") || tx_hash.len() != 66 {
            error!("Invalid transaction hash format: {}", tx_hash);
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<Post>::error("Invalid smart contract transaction hash format".to_string())));
        }
        
       
        match service.is_transaction_used(tx_hash).await {
            Ok(true) => {
                error!("Transaction hash already used: {}", tx_hash);
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<Post>::error("The transaction has already been used, please do not resubmit".to_string())));
            }
            Ok(false) => {
                info!("Transaction hash check passed: {}", tx_hash);
            }
            Err(e) => {
                error!("Failed to check transaction status: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<Post>::error("Transaction verification failed".to_string())));
    }
        }
        
      
        match service.verify_blockchain_post_transaction(tx_hash, &request_data.author_address).await {
            Ok(verification) => {
                info!("Blockchain transaction verification succeeded: {:?}", verification);
                
               
                match service.create_post_with_verification(request_data, verification).await {
        Ok(post) => {
            info!("Successfully created post with ID: {}", post.id);
            Ok(HttpResponse::Created().json(ApiResponse::success(post)))
        }
        Err(e) => {
            error!("Failed to create post: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<Post>::error(e.to_string())))
        }
                }
            }
            Err(e) => {
                error!("Blockchain transaction verification failed: {}", e);
                Ok(HttpResponse::BadRequest().json(ApiResponse::<Post>::error(format!("Blockchain transaction verification failed: {}", e))))
            }
        }
        
    } else {
        error!("Missing smart contract transaction hash");
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<Post>::error("Smart contract transaction hash is required".to_string())));
    }
}

pub async fn get_post(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let post_id = path.into_inner();
    let user_address = query.get("user_address").map(|s| s.as_str());
    info!("Getting post with ID: {} for user: {:?}", post_id, user_address);
    
    match service.get_post_with_like_status(&post_id, user_address).await {
        Some(post) => {
            info!("Found post: {}", post.title);
            Ok(HttpResponse::Ok().json(ApiResponse::success(post)))
        }
        None => {
            info!("Post not found: {}", post_id);
            Ok(HttpResponse::NotFound().json(ApiResponse::<Post>::error("Post not found".to_string())))
        }
    }
}

pub async fn add_comment(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    request: web::Json<CreateCommentRequest>,
) -> Result<HttpResponse> {
    let post_id = path.into_inner();
    let mut comment_request = request.into_inner();
    comment_request.post_id = post_id.clone();
    
    info!("Adding comment to post: {}", post_id);
    
    
    if let Some(tx_hash) = &comment_request.blockchain_transaction_hash {
        info!("Verifying smart contract transaction for comment: {}", tx_hash);
        
        
        if !tx_hash.starts_with("0x") || tx_hash.len() != 66 {
            error!("Invalid transaction hash format: {}", tx_hash);
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<Comment>::error("Invalid smart contract transaction hash format".to_string())));
        }
        
       
        match service.is_transaction_used(tx_hash).await {
            Ok(true) => {
                error!("Transaction hash already used: {}", tx_hash);
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<Comment>::error("The transaction has already been used, please do not resubmit".to_string())));
            }
            Ok(false) => {
                info!("Transaction hash check passed: {}", tx_hash);
            }
            Err(e) => {
                error!("Failed to check transaction status: {}", e);
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<Comment>::error("Transaction verification failed".to_string())));
            }
        }
        
     
        match service.verify_blockchain_comment_transaction(tx_hash, &comment_request.author_address).await {
            Ok(verification) => {
                info!("Comment blockchain transaction verification succeeded: {:?}", verification);
                
          
                match service.add_comment_with_verification(comment_request, verification).await {
                    Ok(comment) => {
                        info!("Successfully added comment with ID: {}", comment.id);
                        Ok(HttpResponse::Created().json(ApiResponse::success(comment)))
                    }
                    Err(e) => {
                        error!("Failed to add comment: {}", e);
                        Ok(HttpResponse::InternalServerError().json(ApiResponse::<Comment>::error(e.to_string())))
                    }
                }
            }
            Err(e) => {
                error!("Comment blockchain transaction verification failed: {}", e);
                Ok(HttpResponse::BadRequest().json(ApiResponse::<Comment>::error(format!("Blockchain transaction verification failed: {}", e))))
            }
        }
    } else {
     
    match service.add_comment(comment_request).await {
        Ok(comment) => {
            info!("Successfully added comment with ID: {}", comment.id);
            Ok(HttpResponse::Created().json(ApiResponse::success(comment)))
        }
        Err(e) => {
            error!("Failed to add comment: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<Comment>::error(e.to_string())))
            }
        }
    }
}

pub async fn get_post_comments(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let post_id = path.into_inner();
    let user_address = query.get("user_address").map(|s| s.as_str());
    
    
    let limit = query.get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(50); 
    
    let offset = query.get("offset")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    
    info!("Getting comments for post: {} (user: {:?}, limit: {}, offset: {})", post_id, user_address, limit, offset);
    
    match service.get_comments_with_like_status_paginated(&post_id, user_address, limit, offset).await {
        Ok(comments) => {
            info!("Retrieved {} comments for post: {}", comments.len(), post_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(comments)))
        }
        Err(e) => {
            error!("Failed to get comments for post {}: {}", post_id, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<Vec<Comment>>::error(format!("Failed to get comments: {}", e))))
        }
    }
}

pub async fn get_user_profile(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let address = path.into_inner();
    info!("Getting user profile for address: {}", address);
    
    match service.get_user_profile(&address).await {
        Some(user) => {
            info!("Found user profile for: {}", address);
            Ok(HttpResponse::Ok().json(ApiResponse::success(user)))
        }
        None => {
            info!("User profile not found: {}", address);
            Ok(HttpResponse::NotFound().json(ApiResponse::<User>::error("User not found".to_string())))
        }
    }
}

pub async fn upload_to_irys(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<IrysUploadRequest>,
) -> Result<HttpResponse> {
    info!("Uploading data to Irys for address: {}", request.address);
    
    match service.upload_to_irys(request.into_inner()).await {
        Ok(tx_id) => {
            info!("Successfully uploaded to Irys with transaction ID: {}", tx_id);
            Ok(HttpResponse::Ok().json(ApiResponse::success(tx_id)))
        }
        Err(e) => {
            error!("Failed to upload to Irys: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<String>::error(e.to_string())))
        }
    }
}

pub async fn query_irys(
    service: web::Data<Arc<ForumService>>,
    query: web::Query<IrysQueryRequest>,
) -> Result<HttpResponse> {
    let query_params = query.into_inner();
    info!("Querying Irys data");
    
    match service.query_irys(query_params.address, query_params.tags, query_params.limit).await {
        Ok(data) => {
            info!("Successfully queried Irys data, found {} items", data.len());
            Ok(HttpResponse::Ok().json(ApiResponse::success(data)))
        }
        Err(e) => {
            error!("Failed to query Irys: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<Vec<Value>>::error(e.to_string())))
        }
    }
}


pub async fn get_active_users_ranking(
    service: web::Data<Arc<ForumService>>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let limit = query.get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(10);
    
    info!("Getting active users ranking, limit: {}", limit);
    
    let users = service.get_active_users_ranking(limit).await;
    info!("Retrieved {} active users", users.len());
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(users)))
}

// 获取全局统计数据
pub async fn get_global_stats(
    service: web::Data<Arc<ForumService>>,
) -> Result<HttpResponse> {
    info!("Getting global statistics");
    
    let stats = service.get_global_stats().await;
    info!("Retrieved global stats: {:?}", stats);
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(stats)))
}

// 点赞帖子
pub async fn like_post(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    request: web::Json<LikeRequest>,
) -> Result<HttpResponse> {
    let post_id = path.into_inner();
    info!("Liking post: {}, user: {}", post_id, request.user_address);
    
    match service.like_post(&post_id, &request.user_address).await {
        Ok(new_likes_count) => {
            info!("Post {} liked successfully, new count: {}", post_id, new_likes_count);
            Ok(HttpResponse::Ok().json(ApiResponse::success(new_likes_count)))
        }
        Err(e) => {
            error!("Failed to like post {}: {}", post_id, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<u32>::error(format!("Failed to like post: {}", e))))
        }
    }
}

// 注册用户名
pub async fn register_username(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<RegisterUsernameRequest>,
) -> Result<HttpResponse> {
    info!("Registering username: {} for address: {}", request.username, request.user_address);
    
    match service.register_username(&request.user_address, &request.username).await {
        Ok(true) => {
            info!("Username {} registered successfully for {}", request.username, request.user_address);
            Ok(HttpResponse::Ok().json(ApiResponse::success("✅ Username registered successfully")))
        }
        Ok(false) => {
            info!("Username {} registration failed for {}", request.username, request.user_address);
            Ok(HttpResponse::BadRequest().json(ApiResponse::<String>::error("Username already exists or you already have a username".to_string())))
        }
        Err(e) => {
            error!("Failed to register username {}: {}", request.username, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<String>::error(format!("Failed to register username: {}", e))))
        }
    }
}

// 检查用户名是否可用
pub async fn check_username(
    service: web::Data<Arc<ForumService>>,
    query: web::Query<CheckUsernameRequest>,
) -> Result<HttpResponse> {
    let username = &query.username;
    
    match service.is_username_available(username).await {
        Ok(available) => {
            let response = if available {
                UsernameCheckResponse {
                    available: true,
                    message: "✅ Username is available".to_string(),
                }
            } else {
                UsernameCheckResponse {
                    available: false,
                    message: "❌ Username is not available or has invalid format".to_string(),
                }
            };
            Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Failed to check username {}: {}", username, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<UsernameCheckResponse>::error(format!("Failed to check username: {}", e))))
        }
    }
}


pub async fn get_username(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let address = path.into_inner();
    
    match service.get_username_by_address(&address).await {
        Ok(username) => {
            Ok(HttpResponse::Ok().json(ApiResponse::success(username)))
        }
        Err(e) => {
            error!("Failed to get username for address {}: {}", address, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<Option<String>>::error(format!("Failed to get username: {}", e))))
        }
    }
}


pub async fn check_user_has_username(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let address = path.into_inner();
    
    match service.user_has_username(&address).await {
        Ok(has_username) => {
            Ok(HttpResponse::Ok().json(ApiResponse::success(has_username)))
        }
        Err(e) => {
            error!("Failed to check username status for address {}: {}", address, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<bool>::error(format!("Failed to check username status: {}", e))))
        }
    }
}


pub async fn sync_user_username(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<SyncUsernameRequest>,
) -> Result<HttpResponse> {
    info!("Syncing username for address: {}", request.user_address);
    
    match service.get_username_by_address(&request.user_address).await {
        Ok(Some(username)) => {
            info!("Successfully synced username {} for {}", username, request.user_address);
            Ok(HttpResponse::Ok().json(ApiResponse::success(format!("✅ Username synced: {}", username))))
        }
        Ok(None) => {
            info!("No username found for {}", request.user_address);
            Ok(HttpResponse::Ok().json(ApiResponse::success("ℹ️ No username registered for this address")))
        }
        Err(e) => {
            error!("Failed to sync username for {}: {}", request.user_address, e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<String>::error(format!("Failed to sync username: {}", e))))
        }
    }
}


pub async fn debug_static_files() -> Result<HttpResponse> {
    use std::fs;
    use std::path::Path;
    
    let static_dir = Path::new("./static");
    let mut files = Vec::new();
    
    if let Ok(entries) = fs::read_dir(static_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Some(file_name) = entry.file_name().to_str() {
                    files.push(file_name.to_string());
                }
            }
        }
    }
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(files)))
}


pub async fn get_performance_stats(
    service: web::Data<Arc<ForumService>>,
) -> impl Responder {
    let mut stats = serde_json::json!({
        "timestamp": chrono::Utc::now(),
        "status": "healthy"
    });
    

    if let Some(db_stats) = service.get_database_performance() {
        stats["database"] = db_stats;
    }
    

    if service.has_cache_service() {
        stats["cache"] = serde_json::json!({
            "status": "active",
            "type": "redis"
        });
    }
    

    stats["memory"] = service.get_memory_stats();
    
    HttpResponse::Ok().json(ApiResponse::success(stats))
}


pub async fn create_post_async(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<CreatePostRequest>,
) -> impl Responder {
    let request_data = request.into_inner();
    
 
    if request_data.title.trim().is_empty() {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("The title of the post cannot be empty".to_string()));
    }
    
    if request_data.content.trim().is_empty() {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("The content of the post cannot be empty".to_string()));
    }
    
    if request_data.blockchain_transaction_hash.is_none() {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("Lack of blockchain transaction hash".to_string()));
    }
    
   
    match service.create_post_async(request_data).await {
        Ok(task_id) => {
            info!("🚀 Post creation task submitted: {}", task_id);
            HttpResponse::Accepted().json(ApiResponse::success(serde_json::json!({
                "task_id": task_id,
                "message": "🚀 Post creation task submitted, processing in background",
                "status_url": format!("/api/tasks/{}", task_id)
            })))
        },
        Err(e) => {
            error!("Failed to submit post creation task: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn create_comment_async(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<CreateCommentRequest>,
) -> impl Responder {
    let request_data = request.into_inner();
    
   
    if request_data.content.trim().is_empty() {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("评论内容不能为空".to_string()));
    }
    
    if request_data.post_id.trim().is_empty() {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("帖子ID不能为空".to_string()));
    }
    
    if request_data.blockchain_transaction_hash.is_none() {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少区块链交易哈希".to_string()));
    }
    
   
    match service.create_comment_async(request_data).await {
        Ok(task_id) => {
            info!("🚀 Comment creation task submitted: {}", task_id);
            HttpResponse::Accepted().json(ApiResponse::success(serde_json::json!({
                "task_id": task_id,
                "message": "🚀 Comment creation task submitted, processing in background",
                "status_url": format!("/api/tasks/{}", task_id)
            })))
        },
        Err(e) => {
            error!("Failed to submit comment creation task: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn like_comment(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    req: web::Json<serde_json::Value>,
) -> impl Responder {
    let comment_id = path.into_inner();
    let user_address = req.get("user_address")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    info!("❤️ User liked comment: {} -> {}", user_address, comment_id);
    
    match service.like_comment(&comment_id, user_address).await {
        Ok((likes, is_new_like)) => {
            if is_new_like {
                info!("✅ Comment liked: {} (new likes: {})", comment_id, likes);
                HttpResponse::Ok().json(ApiResponse::success(json!({
                    "comment_id": comment_id,
                    "likes": likes,
                    "message": "✅ Liked!",
                    "is_new_like": true,
                    "action": "like"
                })))
            } else {
                info!("🔄 Unliked: {} (current likes: {})", comment_id, likes);
                HttpResponse::Ok().json(ApiResponse::success(json!({
                    "comment_id": comment_id,
                    "likes": likes,
                    "message": "🔄 Unliked",
                    "is_new_like": false,
                    "action": "unlike"
                })))
            }
        }
        Err(e) => {
            error!("❌ Failed to like comment: {} - {}", comment_id, e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("点赞失败: {}", e)))
        }
    }
}

//Query asynchronous task status API endpoint
pub async fn get_task_status(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
) -> impl Responder {
    let task_id = path.into_inner();
    
    match service.get_task_status(&task_id).await {
        Some(status) => {
            HttpResponse::Ok().json(ApiResponse::success(status))
        },
        None => {
            HttpResponse::NotFound().json(ApiResponse::<()>::error("Task not found".to_string()))
        }
    }
}

//Get user's own posts
pub async fn get_user_posts(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let user_address = path.into_inner();
    
    
    if user_address.is_empty() || !user_address.starts_with("0x") || user_address.len() != 42 {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("Invalid user address".to_string()));
    }
    
    
    let limit = query.get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20);
    
    let offset = query.get("offset")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    
  
    let request_user_address = query.get("user_address");
    
    match service.get_user_posts_with_like_status(&user_address, limit, offset, request_user_address.map(|s| s.as_str())).await {
        Ok(posts) => {
            info!("👤 Retrieved user posts: {} (count: {})", user_address, posts.len());
            HttpResponse::Ok().json(ApiResponse::success(posts))
        },
        Err(e) => {
            error!("Failed to get user posts: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn follow_user(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<FollowRequest>,
) -> impl Responder {
    info!("👥 Follow request: {:?} -> {:?}", request.follower_address, request.following_address);
    
    match service.follow_user(request.into_inner()).await {
        Ok(response) => {
            info!("✅ Follow operation completed: success={}", response.success);
            HttpResponse::Ok().json(ApiResponse::success(response))
        },
        Err(e) => {
            error!("❌ Follow operation failed: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn unfollow_user(
    service: web::Data<Arc<ForumService>>,
    request: web::Json<FollowRequest>,
) -> impl Responder {
    info!("👥 Unfollow request: {:?} -> {:?}", request.follower_address, request.following_address);
    
    match service.unfollow_user(request.into_inner()).await {
        Ok(response) => {
            info!("✅ Unfollow operation completed: success={}", response.success);
            HttpResponse::Ok().json(ApiResponse::success(response))
        },
        Err(e) => {
            error!("❌ Unfollow operation failed: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn get_following_list(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let user_address = path.into_inner();
    let limit = query.get("limit").and_then(|s| s.parse::<u32>().ok()).unwrap_or(20);
    let offset = query.get("offset").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    
    info!("📋 Get following list: {} (limit: {}, offset: {})", user_address, limit, offset);
    
    match service.get_following_list(&user_address, limit, offset).await {
        Ok(profiles) => {
            info!("✅ Following list fetched: {} (count: {})", user_address, profiles.len());
            HttpResponse::Ok().json(ApiResponse::success(profiles))
        },
        Err(e) => {
            error!("❌ Failed to get following list: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn get_followers_list(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let user_address = path.into_inner();
    let limit = query.get("limit").and_then(|s| s.parse::<u32>().ok()).unwrap_or(20);
    let offset = query.get("offset").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    
    info!("📋 Get followers list: {} (limit: {}, offset: {})", user_address, limit, offset);
    
    match service.get_followers_list(&user_address, limit, offset).await {
        Ok(profiles) => {
            info!("✅ Followers list fetched: {} (count: {})", user_address, profiles.len());
            HttpResponse::Ok().json(ApiResponse::success(profiles))
        },
        Err(e) => {
            error!("❌ Failed to get followers list: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn get_mutual_follows_list(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let user_address = path.into_inner();
    let limit = query.get("limit").and_then(|s| s.parse::<u32>().ok()).unwrap_or(20);
    let offset = query.get("offset").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    
    info!("📋 Get mutual follows list: {} (limit: {}, offset: {})", user_address, limit, offset);
    
    match service.get_mutual_follows_list(&user_address, limit, offset).await {
        Ok(profiles) => {
            info!("✅ Mutual follows list fetched: {} (count: {})", user_address, profiles.len());
            HttpResponse::Ok().json(ApiResponse::success(profiles))
        },
        Err(e) => {
            error!("❌ Failed to get mutual follows list: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


pub async fn check_follow_status(
    service: web::Data<Arc<ForumService>>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {

    let (follower_address, following_address) = if let (Some(follower_id), Some(following_id)) = 
        (query.get("follower_id"), query.get("following_id")) {
   
        match (service.get_user_address_by_id(follower_id).await, service.get_user_address_by_id(following_id).await) {
            (Ok(follower_addr), Ok(following_addr)) => (follower_addr, following_addr),
            _ => {
                return HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的用户ID".to_string()));
            }
        }
    } else if let (Some(follower_addr), Some(following_addr)) = 
        (query.get("follower"), query.get("following")) {
       
        (follower_addr.clone(), following_addr.clone())
    } else {
        return HttpResponse::BadRequest().json(ApiResponse::<()>::error("Missing parameters: require follower_id and following_id, or follower and following".to_string()));
    };
    
    match service.is_following(&follower_address, &following_address).await {
        Ok(is_following) => {
            HttpResponse::Ok().json(ApiResponse::success(json!({
                "is_following": is_following
            })))
        },
        Err(e) => {
            error!("❌ Failed to check follow status: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}

//Obtain user attention statistics
pub async fn get_follow_stats(
    service: web::Data<Arc<ForumService>>,
    path: web::Path<String>,
) -> impl Responder {
    let user_address = path.into_inner();
    
    match service.get_follow_counts(&user_address).await {
        Ok((following_count, followers_count, mutual_follows_count)) => {
            HttpResponse::Ok().json(ApiResponse::success(json!({
                "following_count": following_count,
                "followers_count": followers_count,
                "mutual_follows_count": mutual_follows_count
            })))
        },
        Err(e) => {
            error!("❌ Failed to get follow stats: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e.to_string()))
        }
    }
}


#[derive(Deserialize)]
pub struct AvatarUploadData {
    pub user_address: String,
}


#[derive(Deserialize)]
pub struct BioUpdateRequest {
    pub user_address: String,
    pub bio: String,
}


pub async fn upload_avatar(
    service: web::Data<Arc<ForumService>>,
    mut payload: Multipart,
) -> Result<HttpResponse> {
    let mut user_address = String::new();
    let mut file_data = Vec::new();
    let mut file_name = String::new();
    let mut content_type = String::new();

 
    while let Some(mut field) = payload.try_next().await? {
        let field_name = field.name().to_string();
        
        match field_name.as_str() {
            "user_address" => {
                while let Some(chunk) = field.try_next().await? {
                    user_address.push_str(&String::from_utf8_lossy(&chunk));
                }
            }
            "avatar" => {
             
                let content_disposition = field.content_disposition();
                if let Some(filename) = content_disposition.get_filename() {
                    file_name = filename.to_string();
                }
                
                if let Some(ct) = field.content_type() {
                    content_type = ct.to_string();
                }
                
           
                if !["image/jpeg", "image/jpg", "image/png"].contains(&content_type.as_str()) {
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("Only JPG and PNG images are supported".to_string())));
                }
                
          
                while let Some(chunk) = field.try_next().await? {
                    file_data.extend_from_slice(&chunk);
                }
                
              
                if file_data.len() > 5 * 1024 * 1024 {
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("Image size must not exceed 5MB".to_string())));
                }
            }
            _ => {
            
                while let Some(_chunk) = field.try_next().await? {}
            }
        }
    }

    if user_address.is_empty() || file_data.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("Missing required parameters".to_string())));
    }

    info!("📤 Avatar upload request: user={}, file_size={} bytes", user_address, file_data.len());


    let file_extension = if content_type == "image/png" { "png" } else { "jpg" };
    let new_filename = format!("avatar_{}_{}.{}", user_address, Uuid::new_v4(), file_extension);
    
 
    std::fs::create_dir_all("static/avatars").map_err(|e| {
        error!("Failed to create avatars directory: {}", e);
        actix_web::error::ErrorInternalServerError("Filesystem error")
    })?;
    
 
    let file_path = format!("static/avatars/{}", new_filename);
    let mut file = std::fs::File::create(&file_path).map_err(|e| {
        error!("Failed to create avatar file: {}", e);
        actix_web::error::ErrorInternalServerError("File save failed")
    })?;
    
    file.write_all(&file_data).map_err(|e| {
        error!("Failed to write avatar file: {}", e);
        actix_web::error::ErrorInternalServerError("File write failed")
    })?;

    let avatar_url = format!("/avatars/{}", new_filename);
    
   
    match service.update_user_avatar(&user_address, &avatar_url).await {
        Ok(_) => {
            info!("✅ Avatar uploaded successfully: {}", avatar_url);
            Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
                "avatar_url": avatar_url
            }))))
        }
        Err(e) => {
            error!("❌ Failed to update user avatar: {}", e);
        
            let _ = std::fs::remove_file(&file_path);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to update avatar".to_string())))
        }
    }
}


pub async fn update_bio(
    service: web::Data<Arc<ForumService>>,
    bio_data: web::Json<BioUpdateRequest>,
) -> Result<HttpResponse> {
    let user_address = &bio_data.user_address;
    let bio = &bio_data.bio;
    
  
    if bio.len() > 500 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("Bio must be 500 characters or less".to_string())));
    }

    info!("📝 Update personal profile request: User={}, profile length={}", user_address, bio.len());

    match service.update_user_bio(user_address, bio).await {
        Ok(_) => {
            info!("✅ Personal profile updated successfully");
            Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
                "message": "✅ Bio updated successfully"
            }))))
        }
        Err(e) => {
            error!("❌ Failed to update personal profile: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to update bio".to_string())))
        }
    }
}

//Get daily recommendations
pub async fn get_daily_recommendations(
    service: web::Data<Arc<ForumService>>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let user_address = query.get("user_address").map(|s| s.as_str());
    
    info!("📊 Get daily recommendations request (user: {:?})", user_address);
    
    match service.get_daily_recommendations(user_address).await {
        Ok(result) => {
            info!("✅ Daily recommendations fetched, {} posts returned", result.posts.len());
            Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
                "posts": result.posts,
                "last_refresh_time": result.last_refresh_time
            }))))
        }
        Err(e) => {
            error!("❌ Failed to obtain daily recommendations: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to get daily recommendations".to_string())))
        }
    }
}

// Proxy for Kaito Irys API to avoid CORS issues
pub async fn get_amplifiers(
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let window = query.get("window").map(|s| s.as_str()).unwrap_or("7d");

    info!("Fetching Irys amplifiers data for window: {}", window);

    let url = format!("https://kaito.irys.xyz/api/community-mindshare?window={}", window);

    match reqwest::get(&url).await {
        Ok(response) => {
            match response.json::<Value>().await {
                Ok(data) => {
                    info!("Successfully fetched amplifiers data");
                    Ok(HttpResponse::Ok().json(data))
                }
                Err(e) => {
                    error!("Failed to parse amplifiers data: {}", e);
                    Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to parse data".to_string())))
                }
            }
        }
        Err(e) => {
            error!("Failed to fetch amplifiers data: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to fetch data".to_string())))
        }
    }
}