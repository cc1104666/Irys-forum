use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::models::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueueTask {
    PostCreation {
        task_id: String,
        request: CreatePostRequest,
        tx_hash: String,
        user_address: String,
        timestamp: DateTime<Utc>,
    },
    CommentCreation {
        task_id: String,
        request: CreateCommentRequest,
        tx_hash: String,
        user_address: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Processing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result_data: Option<serde_json::Value>,
}

pub struct AsyncQueueService {
    task_sender: mpsc::UnboundedSender<QueueTask>,
    task_status: Arc<RwLock<HashMap<String, TaskResult>>>,
    blockchain_service: Option<Arc<crate::blockchain::BlockchainService>>,
    database_service: Option<Arc<crate::database::DatabaseService>>,
}

impl AsyncQueueService {
    pub fn new(
        blockchain_service: Option<Arc<crate::blockchain::BlockchainService>>,
        database_service: Option<Arc<crate::database::DatabaseService>>,
    ) -> Self {
        let (task_sender, task_receiver) = mpsc::unbounded_channel();
        let task_status = Arc::new(RwLock::new(HashMap::new()));
        
        let service = Self {
            task_sender,
            task_status: task_status.clone(),
            blockchain_service,
            database_service,
        };
        
        
        service.start_task_processor(task_receiver, task_status);
        service
    }
    
    /// Submit async post creation task
    pub async fn submit_post_creation(
        &self,
        request: CreatePostRequest,
        tx_hash: String,
    ) -> Result<String, String> {
        let task_id = Uuid::new_v4().to_string();
        let task = QueueTask::PostCreation {
            task_id: task_id.clone(),
            request,
            tx_hash,
            user_address: "".to_string(),
            timestamp: Utc::now(),
        };
        
        
        {
            let mut status_map = self.task_status.write().await;
            status_map.insert(task_id.clone(), TaskResult {
                task_id: task_id.clone(),
                status: TaskStatus::Pending,
                created_at: Utc::now(),
                completed_at: None,
                result_data: None,
            });
        }
        
        
        self.task_sender.send(task).map_err(|e| format!("Queue send failed: {}", e))?;
        Ok(task_id)
    }
    
    pub async fn submit_comment_creation(
        &self,
        request: CreateCommentRequest,
        tx_hash: String,
    ) -> Result<String, String> {
        let task_id = Uuid::new_v4().to_string();
        let task = QueueTask::CommentCreation {
            task_id: task_id.clone(),
            request,
            tx_hash,
            user_address: "".to_string(),
            timestamp: Utc::now(),
        };
        
        
        {
            let mut status_map = self.task_status.write().await;
            status_map.insert(task_id.clone(), TaskResult {
                task_id: task_id.clone(),
                status: TaskStatus::Pending,
                created_at: Utc::now(),
                completed_at: None,
                result_data: None,
            });
        }
        
        self.task_sender.send(task).map_err(|e| format!("Queue send failed: {}", e))?;
        Ok(task_id)
    }
    
    /// Query task status by task_id
    pub async fn get_task_status(&self, task_id: &str) -> Option<TaskResult> {
        let status_map = self.task_status.read().await;
        status_map.get(task_id).cloned()
    }
    
    
    fn start_task_processor(
        &self,
        mut task_receiver: mpsc::UnboundedReceiver<QueueTask>,
        task_status: Arc<RwLock<HashMap<String, TaskResult>>>,
    ) {
        let blockchain_service = self.blockchain_service.clone();
        let database_service = self.database_service.clone();
        
        tokio::spawn(async move {
            
            let worker_count = std::env::var("ASYNC_WORKER_COUNT")
                .unwrap_or_else(|_| "10".to_string())
                .parse::<usize>()
                .unwrap_or(10);
            
            let (work_sender, work_receiver) = mpsc::unbounded_channel();
            let work_receiver = Arc::new(tokio::sync::Mutex::new(work_receiver));
            
            
            for worker_id in 0..worker_count {
                let work_receiver = work_receiver.clone();
                let task_status = task_status.clone();
                let blockchain_service = blockchain_service.clone();
                let database_service = database_service.clone();
                
                tokio::spawn(async move {
                    Self::worker_loop(
                        worker_id,
                        work_receiver,
                        task_status,
                        blockchain_service,
                        database_service,
                    ).await;
                });
            }
            
            
            while let Some(task) = task_receiver.recv().await {
                if let Err(_) = work_sender.send(task) {
                    log::error!("Work queue closed");
                    break;
                }
            }
        });
    }
    
    async fn worker_loop(
        worker_id: usize,
        work_receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<QueueTask>>>,
        task_status: Arc<RwLock<HashMap<String, TaskResult>>>,
        blockchain_service: Option<Arc<crate::blockchain::BlockchainService>>,
        database_service: Option<Arc<crate::database::DatabaseService>>,
    ) {
        log::info!("🔧 Worker {} started", worker_id);
        
        loop {
            let task = {
                let mut receiver = work_receiver.lock().await;
                receiver.recv().await
            };
            
            match task {
                Some(task) => {
                    log::info!("🔄 Worker {} processing task: {:?}", worker_id, task);
                    Self::process_task(
                        task,
                        task_status.clone(),
                        blockchain_service.clone(),
                        database_service.clone(),
                    ).await;
                }
                None => {
                    log::info!("🛑 Worker {} received shutdown signal", worker_id);
                    break;
                }
            }
        }
    }
    
    async fn process_task(
        task: QueueTask,
        task_status: Arc<RwLock<HashMap<String, TaskResult>>>,
        blockchain_service: Option<Arc<crate::blockchain::BlockchainService>>,
        database_service: Option<Arc<crate::database::DatabaseService>>,
    ) {
        let task_id = match &task {
            QueueTask::PostCreation { task_id, .. } => task_id.clone(),
            QueueTask::CommentCreation { task_id, .. } => task_id.clone(),
        };
        
        
        {
            let mut status_map = task_status.write().await;
            if let Some(status) = status_map.get_mut(&task_id) {
                status.status = TaskStatus::Processing;
            }
        }
        
        let result = match task {
            QueueTask::PostCreation { request, tx_hash, .. } => {
                Self::process_post_creation(
                    request,
                    tx_hash,
                    blockchain_service,
                    database_service,
                ).await
            }
            QueueTask::CommentCreation { request, tx_hash, .. } => {
                Self::process_comment_creation(
                    request,
                    tx_hash,
                    blockchain_service,
                    database_service,
                ).await
            }
        };
        
        
        {
            let mut status_map = task_status.write().await;
            if let Some(status) = status_map.get_mut(&task_id) {
                status.completed_at = Some(Utc::now());
                match result {
                    Ok(data) => {
                        status.status = TaskStatus::Completed;
                        status.result_data = Some(data);
                    }
                    Err(error) => {
                        status.status = TaskStatus::Failed(error);
                    }
                }
            }
        }
    }
    
    async fn process_post_creation(
        request: CreatePostRequest,
        tx_hash: String,
        blockchain_service: Option<Arc<crate::blockchain::BlockchainService>>,
        database_service: Option<Arc<crate::database::DatabaseService>>,
    ) -> Result<serde_json::Value, String> {
        log::info!("🔄 Starting async post creation: {}", tx_hash);
        
        
        if let Some(blockchain) = blockchain_service {
            let verification = blockchain
                .verify_post_transaction(&tx_hash, &request.author_address)
                .await
                .map_err(|e| format!("Blockchain verification failed: {}", e))?;
            
            log::info!("✅ Blockchain verification succeeded: {}", verification.transaction_hash);
            

            if let Some(database) = database_service {
         
                match database.is_transaction_used(&tx_hash).await {
                    Ok(true) => {
                        return Err("The transaction hash has been used".to_string());
                    }
                    Ok(false) => {
                        log::info!("✅ Transaction hash verification passed");
                    }
                    Err(e) => {
                        log::warn!("⚠️ Transaction verification failed, continue processing: {}", e);
                    }
                }
                
   
                let post_id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now();
                
                let post = crate::models::Post {
                    id: post_id.clone(),
                    title: request.title.clone(),
                    content: request.content.clone(),
                    author_address: request.author_address.clone(),
                    author_id: None, 
                    author_name: request.author_name.clone(),
                    author_avatar: None,
                    created_at: now,
                    updated_at: now,
                    likes: 0,
                    comments_count: 0,
                    tags: request.tags.clone(),
                    irys_transaction_id: None,
                    image: request.image.clone(),
                    blockchain_post_id: request.blockchain_post_id,
                    is_liked_by_user: false, 
                    views: 0, 
                    heat_score: None, 
                };
                
                //Save post to database
                database.create_post(&post).await
                    .map_err(|e| format!("Database save failed: {}", e))?;
                
               
                database.update_post_blockchain_hash(&post_id, &tx_hash).await
                    .map_err(|e| format!("Failed to update transaction hash: {}", e))?;
                
                
                let block_timestamp = chrono::DateTime::from_timestamp(
                    verification.block_timestamp.as_u64() as i64, 0
                ).unwrap_or_else(|| chrono::Utc::now());
                
                database.record_post_transaction(
                    &tx_hash,
                    &verification.sender,
                    verification.block_number,
                    block_timestamp,
                    &post_id
                ).await.map_err(|e| format!("Record transaction failure: {}", e))?;
                
                log::info!("✅ Post asynchronous creation completed: {}", post_id);
                
                return Ok(serde_json::json!({
                    "success": true, 
                    "message": "Post created successfully", 
                    "post_id": post_id,
                    "transaction_hash": tx_hash
                }));
            }
        }
        
        Err("Blockchain service or database service unavailable".to_string())
    }
    
    async fn process_comment_creation(
        request: CreateCommentRequest,
        tx_hash: String,
        blockchain_service: Option<Arc<crate::blockchain::BlockchainService>>,
        database_service: Option<Arc<crate::database::DatabaseService>>,
    ) -> Result<serde_json::Value, String> {
        log::info!("🔄 Start asynchronous processing of comment creation: {}", tx_hash);
        
        
        if let Some(blockchain) = blockchain_service {
            let verification = blockchain
                .verify_comment_transaction(&tx_hash, &request.author_address)
                .await
                .map_err(|e| format!("Blockchain verification failed: {}", e))?;
            
            log::info!("✅ Blockchain verification succeeded: {}", verification.transaction_hash);
            
         
            if let Some(database) = database_service {
                
                match database.is_transaction_used(&tx_hash).await {
                    Ok(true) => {
                        return Err("The transaction hash has been used".to_string());
                    }
                    Ok(false) => {
                        log::info!("✅ Transaction hash verification passed");
                    }
                    Err(e) => {
                        log::warn!("⚠️ Transaction verification failed, continue processing: {}", e);
                    }
                }
                
               
                match database.check_duplicate_comment(&request.author_address, &request.content, &request.post_id).await {
                    Ok(true) => {
                        return Err("You have posted a comment with the same content in the last 5 minutes".to_string());
                    }
                    Ok(false) => {
                        log::info!("✅ Content re check passed");
                    }
                    Err(e) => {
                        log::warn!("⚠️ Content re check failed, continue processing: {}", e);
                    }
                }
                
                
                let comment_id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now();
                let content_hash = {
                    use sha2::{Sha256, Digest};
                    let mut hasher = Sha256::new();
                    hasher.update(request.content.as_bytes());
                    format!("{:x}", hasher.finalize())
                };
                
                let comment = crate::models::Comment {
                    id: comment_id.clone(),
                    post_id: request.post_id.clone(),
                    content: request.content.clone(),
                    author_address: request.author_address.clone(),
                    author_id: None, 
                    author_name: request.author_name.clone(),
                    author_avatar: None,
                    created_at: now,
                    parent_id: request.parent_id.clone(),
                    likes: 0,
                    irys_transaction_id: None,
                    image: request.image.clone(),
                    content_hash,
                    is_liked_by_user: false,
                };
                
                
                database.add_comment(&comment).await
                    .map_err(|e| format!("Database save failed: {}", e))?;
                
               
                database.update_comment_blockchain_hash(&comment_id, &tx_hash).await
                    .map_err(|e| format!("Failed to update transaction hash: {}", e))?;
                
                
                let block_timestamp = chrono::DateTime::from_timestamp(
                    verification.block_timestamp.as_u64() as i64, 0
                ).unwrap_or_else(|| chrono::Utc::now());
                
                database.record_comment_transaction(
                    &tx_hash,
                    &verification.sender,
                    verification.block_number,
                    block_timestamp,
                    &comment_id
                ).await.map_err(|e| format!("Record transaction failure: {}", e))?;
                
                log::info!("✅ Asynchronous creation of comments completed: {}", comment_id);
                
                return Ok(serde_json::json!({
                    "success": true, 
                    "message": "Comment created successfully", 
                    "comment_id": comment_id,
                    "post_id": request.post_id,
                    "transaction_hash": tx_hash
                }));
            }
        }
        
        Err("Blockchain service or database service unavailable".to_string())
    }
} 