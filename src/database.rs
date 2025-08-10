use crate::models::*;
use chrono::Utc;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;
use log::{info, warn};
use unicode_normalization::UnicodeNormalization;
use md5;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Database performance stats
#[derive(Debug)]
pub struct DatabaseStats {
    pub total_queries: AtomicU64,
    pub total_query_time: AtomicU64,
    pub failed_queries: AtomicU64,
    pub connection_pool_waits: AtomicU64,
}

impl DatabaseStats {
    pub fn new() -> Self {
        Self {
            total_queries: AtomicU64::new(0),
            total_query_time: AtomicU64::new(0),
            failed_queries: AtomicU64::new(0),
            connection_pool_waits: AtomicU64::new(0),
        }
    }
    
    pub fn record_query(&self, duration: Duration, success: bool) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.total_query_time.fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        
        if !success {
            self.failed_queries.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    pub fn get_avg_query_time(&self) -> f64 {
        let total_queries = self.total_queries.load(Ordering::Relaxed);
        if total_queries == 0 {
            return 0.0;
        }
        let total_time = self.total_query_time.load(Ordering::Relaxed);
        total_time as f64 / total_queries as f64
    }
}

#[derive(Clone)]
pub struct DatabaseService {
    pool: PgPool,
    stats: Arc<DatabaseStats>,
}

impl DatabaseService {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        /// Build pool options from environment variables with sensible defaults
        let max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "50".to_string())
            .parse::<u32>()
            .unwrap_or(50);
            
        let min_connections = std::env::var("DATABASE_MIN_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u32>()
            .unwrap_or(10);
        
        /// High performance pool configuration
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .max_lifetime(Duration::from_secs(60 * 60))
            .idle_timeout(Duration::from_secs(15 * 60))
            .acquire_timeout(Duration::from_secs(10))
            .test_before_acquire(true)
            .connect(database_url)
            .await?;
        
        info!("🗄️ Database pool initialized - max connections: {}, min connections: {}", max_connections, min_connections);
        
        /// Connection pool health check
        Self::health_check(&pool).await?;
        
        Ok(Self { 
            pool,
            stats: Arc::new(DatabaseStats::new()),
        })
    }
    
    /// Connection pool health check
    async fn health_check(pool: &PgPool) -> Result<(), sqlx::Error> {
        let start = std::time::Instant::now();
        sqlx::query("SELECT 1")
            .fetch_one(pool)
            .await?;
        let duration = start.elapsed();
        info!("📊 Database health check complete - response time: {:?}", duration);
        Ok(())
    }
    
    /// Get connection pool status
    pub fn get_pool_status(&self) -> String {
        format!(
            "Pool status - total: {}, idle: {}, max: {}",
            self.pool.size(),
            self.pool.num_idle(),
            self.pool.options().get_max_connections()
        )
    }
    
    /// Like or unlike a comment (toggle; prevent duplicate likes)
    pub async fn like_comment(&self, comment_id: &str, user_address: &str) -> Result<(u32, bool), sqlx::Error> {
        let comment_uuid = match Uuid::parse_str(comment_id) {
            Ok(uuid) => uuid,
            Err(_) => return Err(sqlx::Error::RowNotFound),
        };
        
        
        let mut tx = self.pool.begin().await?;
        
        
        let existing_like: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM comment_likes WHERE comment_id = $1 AND user_address = $2"
        )
        .bind(comment_uuid)
        .bind(user_address)
        .fetch_optional(&mut *tx)
        .await?;
        
        if existing_like.is_some() {
            
            sqlx::query(
                "DELETE FROM comment_likes WHERE comment_id = $1 AND user_address = $2"
            )
            .bind(comment_uuid)
            .bind(user_address)
            .execute(&mut *tx)
            .await?;
            
            
            sqlx::query(
                "UPDATE comments SET likes = GREATEST(0, COALESCE(likes, 0) - 1) WHERE id = $1"
            )
            .bind(comment_uuid)
            .execute(&mut *tx)
            .await?;
            
            
            let likes: i32 = sqlx::query_scalar(
                "SELECT COALESCE(likes, 0) FROM comments WHERE id = $1"
            )
            .bind(comment_uuid)
            .fetch_one(&mut *tx)
            .await?;
            
            tx.commit().await?;
            return Ok((likes as u32, false));
        }
        
        
        sqlx::query(
            "INSERT INTO comment_likes (comment_id, user_address) VALUES ($1, $2)"
        )
        .bind(comment_uuid)
        .bind(user_address)
        .execute(&mut *tx)
        .await?;
        
        
        let result = sqlx::query(
            "UPDATE comments SET likes = COALESCE(likes, 0) + 1 WHERE id = $1"
        )
        .bind(comment_uuid)
        .execute(&mut *tx)
        .await?;
        
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        
        
        let likes: i32 = sqlx::query_scalar(
            "SELECT COALESCE(likes, 0) FROM comments WHERE id = $1"
        )
        .bind(comment_uuid)
        .fetch_one(&mut *tx)
        .await?;
        
        
        tx.commit().await?;
        
        Ok((likes as u32, true))
    }
    
    /// Check whether a user has liked a comment
    pub async fn check_comment_liked(&self, comment_id: &str, user_address: &str) -> Result<bool, sqlx::Error> {
        let comment_uuid = match Uuid::parse_str(comment_id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(false),
        };
        
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM comment_likes WHERE comment_id = $1 AND user_address = $2"
        )
        .bind(comment_uuid)
        .bind(user_address)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count > 0)
    }
    
    /// Execute operation and record performance stats
    async fn execute_with_stats<F, R>(&self, operation_name: &str, operation: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let start = Instant::now();
        let result = operation.await;
        let duration = start.elapsed();
        
        
        self.stats.record_query(duration, true);
        
        
        if duration.as_millis() > 100 {
            warn!("⚠️ Slow query detected - {}: {:?}", operation_name, duration);
        }
        
        result
    }
    
    /// Get database performance stats
    pub fn get_database_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_queries": self.stats.total_queries.load(Ordering::Relaxed),
            "total_query_time_ms": self.stats.total_query_time.load(Ordering::Relaxed),
            "failed_queries": self.stats.failed_queries.load(Ordering::Relaxed),
            "avg_query_time_ms": self.stats.get_avg_query_time(),
            "connection_pool_waits": self.stats.connection_pool_waits.load(Ordering::Relaxed),
            "pool_status": self.get_pool_status()
        })
    }

    /// Create a post (transactional for consistency)
    pub async fn create_post(&self, post: &Post) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        
        
        self.ensure_user_exists_tx(&mut tx, &post.author_address, &post.author_name).await?;
        

        let post_uuid = uuid::Uuid::parse_str(&post.id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
        

        sqlx::query(
            r#"
            INSERT INTO posts (id, title, content, author_id, content_hash, category, tags, upvotes, irys_transaction_id, author_name, likes, created_at, updated_at, image, blockchain_post_id)
            SELECT $1, $2, $3, u.id, $4, 'general', $5, $6, $7, $8, $9, $10, $11, $12, $13
            FROM users u WHERE u.ethereum_address = $14
            "#
        )
        .bind(post_uuid)
        .bind(&post.title)
        .bind(&post.content)
        .bind(format!("{:x}", md5::compute(&post.content)))
        .bind(&post.tags)
        .bind(post.likes as i32) 
        .bind(&post.irys_transaction_id)
        .bind(&post.author_name)
        .bind(post.likes as i32) 
        .bind(post.created_at)
        .bind(post.updated_at)
        .bind(&post.image)
        .bind(post.blockchain_post_id.map(|id| id as i32))
        .bind(&post.author_address)
        .execute(&mut *tx)
        .await?;

        
        let update_sql = format!(
            "UPDATE users SET posts_count = COALESCE(posts_count, 0) + 1, reputation = COALESCE(reputation, 0) + 10 WHERE ethereum_address = '{}'",
            post.author_address.replace("'", "''")
        );
        sqlx::query(&update_sql)
        .execute(&mut *tx)
        .await?;
        
        
        tx.commit().await?;
        
        Ok(())
    }

    /// Add a comment to database
    pub async fn add_comment(&self, comment: &Comment) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        
        
        self.ensure_user_exists_tx(&mut tx, &comment.author_address, &comment.author_name).await?;
        
        
        let comment_uuid = uuid::Uuid::parse_str(&comment.id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
        
        
        let post_uuid = uuid::Uuid::parse_str(&comment.post_id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
        
        
        let parent_uuid = if let Some(parent_id) = &comment.parent_id {
            Some(uuid::Uuid::parse_str(parent_id)
                .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?)
        } else {
            None
        };

        
        sqlx::query(
            r#"
            INSERT INTO comments (id, content, author_id, post_id, parent_id, created_at, likes, irys_transaction_id, author_name, image, content_hash, updated_at)
            SELECT $1, $2, u.id, $3, $4, $5, $6, $7, $8, $9, $10, $11
            FROM users u WHERE u.ethereum_address = $12
            "#
        )
        .bind(comment_uuid)
        .bind(&comment.content)
        .bind(post_uuid)
        .bind(parent_uuid)
        .bind(comment.created_at)
        .bind(comment.likes as i32)
        .bind(&comment.irys_transaction_id)
        .bind(&comment.author_name)
        .bind(&comment.image)
        .bind(&comment.content_hash)
        .bind(comment.created_at)
        .bind(&comment.author_address)
        .execute(&mut *tx)
        .await?;

        
        sqlx::query(
            "UPDATE posts SET comments_count = comments_count + 1 WHERE id = $1"
        )
        .bind(post_uuid)
        .execute(&mut *tx)
        .await?;
        
        
        let update_sql = format!(
            "UPDATE users SET comments_count = COALESCE(comments_count, 0) + 1, reputation = COALESCE(reputation, 0) + 5 WHERE ethereum_address = '{}'",
            comment.author_address.replace("'", "''")
        );
        sqlx::query(&update_sql)
        .execute(&mut *tx)
        .await?;
        
        
        tx.commit().await?;
        
        Ok(())
    }

    /// Simplified post query
    pub async fn get_posts(&self) -> Result<Vec<Post>, sqlx::Error> {
        self.get_posts_paginated(1000, 0).await
    }
    
    /// Paginated posts query
    pub async fn get_posts_paginated(&self, limit: u32, offset: u32) -> Result<Vec<Post>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                   (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                   COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                   p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                   COALESCE(p.views, 0) as views,
                   u.id as user_id, u.ethereum_address, 
                   COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar
            FROM posts p
            JOIN users u ON p.author_id = u.id
            ORDER BY p.created_at DESC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut posts = Vec::new();
        for row in rows {
            let post = Post {
                id: row.try_get::<Uuid, _>("id")?.to_string(),
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                author_address: row.try_get::<Option<String>, _>("ethereum_address")?.unwrap_or_default(),
                author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
                author_name: row.try_get("author_name")?,
                author_avatar: row.try_get("author_avatar").ok(),
                created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
                updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
                likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
                comments_count: row.try_get::<i64, _>("comments_count").unwrap_or(0) as u32,
                views: row.try_get::<i32, _>("views").unwrap_or(0) as u32,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
                irys_transaction_id: row.try_get("irys_transaction_id").ok(),
                image: row.try_get("image").ok(),
                blockchain_post_id: row.try_get::<Option<i32>, _>("blockchain_post_id").ok().flatten().map(|id| id as u32),
                is_liked_by_user: false,
                heat_score: None,
            };
            posts.push(post);
        }

        Ok(posts)
    }
    
    /// Get posts by user (paginated)
    pub async fn get_posts_by_user(&self, user_address: &str, limit: u32, offset: u32) -> Result<Vec<Post>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                   (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                   COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                   p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                   COALESCE(p.views, 0) as views,
                   u.id as user_id, u.ethereum_address, 
                   COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar
            FROM posts p
            JOIN users u ON p.author_id = u.id
            WHERE LOWER(u.ethereum_address) = LOWER($1)
            ORDER BY p.created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(user_address)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut posts = Vec::new();
        for row in rows {
            let post = Post {
                id: row.try_get::<Uuid, _>("id")?.to_string(),
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                author_address: row.try_get::<Option<String>, _>("ethereum_address")?.unwrap_or_default(),
                author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
                author_name: row.try_get("author_name")?,
                author_avatar: row.try_get("author_avatar").ok(),
                created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
                updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
                likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
                comments_count: row.try_get::<i64, _>("comments_count").unwrap_or(0) as u32,
                views: row.try_get::<i32, _>("views").unwrap_or(0) as u32,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
                irys_transaction_id: row.try_get("irys_transaction_id").ok(),
                image: row.try_get("image").ok(),
                blockchain_post_id: row.try_get::<Option<i32>, _>("blockchain_post_id").ok().flatten().map(|id| id as u32),
                is_liked_by_user: row.try_get::<bool, _>("is_liked_by_user").unwrap_or(false),
                heat_score: None,
            };
            posts.push(post);
        }

        Ok(posts)
    }

    /// Get posts by user (paginated, with like status)
    pub async fn get_posts_by_user_with_like_status(&self, user_address: &str, limit: u32, offset: u32, request_user_address: Option<&str>) -> Result<Vec<Post>, sqlx::Error> {
        let rows = if let Some(req_addr) = request_user_address {
            // 包含点赞状态的查询
            let query_result = sqlx::query(
                r#"
                SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                       (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                       COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                       p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                       COALESCE(p.views, 0) as views,
                       u.id as user_id, u.ethereum_address, 
                       COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar,
                       CASE WHEN pl.user_address IS NOT NULL THEN true ELSE false END as is_liked_by_user
                FROM posts p
                JOIN users u ON p.author_id = u.id
                LEFT JOIN post_likes pl ON pl.post_id = p.id AND LOWER(pl.user_address) = LOWER($4)
                WHERE LOWER(u.ethereum_address) = LOWER($1)
                ORDER BY p.created_at DESC
                LIMIT $2 OFFSET $3
                "#
            )
            .bind(user_address)
            .bind(limit as i64)
            .bind(offset as i64)
            .bind(req_addr)
            .fetch_all(&self.pool)
            .await?;
            

            println!("🔍 User posts query: target user={}, requester={}", user_address, req_addr);
            

            let like_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM post_likes WHERE LOWER(user_address) = LOWER($1)"
            )
            .bind(req_addr)
            .fetch_one(&self.pool)
            .await.unwrap_or(0);
            
            println!("🔍 Total post-like records for user {}: {}", req_addr, like_count);
            query_result
        } else {

            sqlx::query(
                r#"
                SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                       (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                       COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                       p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                       COALESCE(p.views, 0) as views,
                       u.id as user_id, u.ethereum_address, 
                       COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar,
                       false as is_liked_by_user
                FROM posts p
                JOIN users u ON p.author_id = u.id
                WHERE LOWER(u.ethereum_address) = LOWER($1)
                ORDER BY p.created_at DESC
                LIMIT $2 OFFSET $3
                "#
            )
            .bind(user_address)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };

        let mut posts = Vec::new();
        for row in rows {
            let post = Post {
                id: row.try_get::<Uuid, _>("id")?.to_string(),
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                author_address: row.try_get::<Option<String>, _>("ethereum_address")?.unwrap_or_default(),
                author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
                author_name: row.try_get("author_name")?,
                author_avatar: row.try_get("author_avatar").ok(),
                created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
                updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
                likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
                comments_count: row.try_get::<i64, _>("comments_count").unwrap_or(0) as u32,
                views: row.try_get::<i32, _>("views").unwrap_or(0) as u32,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
                irys_transaction_id: row.try_get("irys_transaction_id").ok(),
                image: row.try_get("image").ok(),
                blockchain_post_id: row.try_get::<Option<i32>, _>("blockchain_post_id").ok().flatten().map(|id| id as u32),
                is_liked_by_user: row.try_get::<bool, _>("is_liked_by_user").unwrap_or(false),
                heat_score: None,
            };
            posts.push(post);
        }

        Ok(posts)
    }

    /// Get single post
    pub async fn get_post_by_id(&self, id: &str) -> Result<Option<Post>, sqlx::Error> {
        let post_uuid = match Uuid::parse_str(id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(None),
        };
        
        let row = sqlx::query(
            r#"
            SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                   (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                   COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                   p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                   COALESCE(p.views, 0) as views,
                   u.id as user_id, u.ethereum_address, 
                   COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar
            FROM posts p
            JOIN users u ON p.author_id = u.id
            WHERE p.id = $1
            "#
        )
        .bind(post_uuid)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| Post {
            id: row.try_get::<Uuid, _>("id").unwrap().to_string(),
            title: row.try_get("title").unwrap(),
            content: row.try_get("content").unwrap(),
            author_address: row.try_get::<Option<String>, _>("ethereum_address").unwrap().unwrap_or_default(),
            author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
            author_name: row.try_get("author_name").unwrap(),
            author_avatar: row.try_get("author_avatar").ok(),
            created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
            likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
            comments_count: row.try_get::<i64, _>("comments_count").unwrap_or(0) as u32,
            views: row.try_get::<i32, _>("views").unwrap_or(0) as u32,
            tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            irys_transaction_id: row.try_get("irys_transaction_id").ok(),
            image: row.try_get("image").ok(),
            blockchain_post_id: row.try_get::<Option<i32>, _>("blockchain_post_id").ok().flatten().map(|id| id as u32),
            is_liked_by_user: false, // 默认为false，后续由service层设置
            heat_score: None,
        }))
    }

    /// Get single post (with user's like status)
    pub async fn get_post_by_id_with_like_status(&self, id: &str, user_address: Option<&str>) -> Result<Option<Post>, sqlx::Error> {
        let post_uuid = match Uuid::parse_str(id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(None),
        };
        
        let row = if let Some(address) = user_address {
            sqlx::query(
                r#"
                SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                       (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                       COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                       p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                       u.id as user_id, u.ethereum_address, 
                       COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar,
                       CASE WHEN pl.user_address IS NOT NULL THEN true ELSE false END as is_liked_by_user
                FROM posts p
                JOIN users u ON p.author_id = u.id
                LEFT JOIN post_likes pl ON pl.post_id = p.id AND pl.user_address = $2
                WHERE p.id = $1
                "#
            )
            .bind(post_uuid)
            .bind(address)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT p.id, p.title, p.content, COALESCE(p.likes, 0) as likes, 
                       (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                       COALESCE(p.tags, '{}') as tags, p.irys_transaction_id, 
                       p.created_at, p.updated_at, p.image, p.blockchain_post_id,
                       u.id as user_id, u.ethereum_address, 
                       COALESCE(p.author_name, u.username) as author_name, u.avatar as author_avatar,
                       false as is_liked_by_user
                FROM posts p
                JOIN users u ON p.author_id = u.id
                WHERE p.id = $1
                "#
            )
            .bind(post_uuid)
            .fetch_optional(&self.pool)
            .await?
        };

        Ok(row.map(|row| Post {
            id: row.try_get::<Uuid, _>("id").unwrap().to_string(),
            title: row.try_get("title").unwrap(),
            content: row.try_get("content").unwrap(),
            author_address: row.try_get::<Option<String>, _>("ethereum_address").unwrap().unwrap_or_default(),
            author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
            author_name: row.try_get("author_name").unwrap(),
            author_avatar: row.try_get("author_avatar").ok(),
            created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
            likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
            comments_count: row.try_get::<i64, _>("comments_count").unwrap_or(0) as u32,
            views: row.try_get::<i32, _>("views").unwrap_or(0) as u32,
            tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
            irys_transaction_id: row.try_get("irys_transaction_id").ok(),
            image: row.try_get("image").ok(),
            blockchain_post_id: row.try_get::<Option<i32>, _>("blockchain_post_id").ok().flatten().map(|id| id as u32),
            is_liked_by_user: row.try_get::<bool, _>("is_liked_by_user").unwrap_or(false),
            heat_score: None,
        }))
    }

    /// Create comment
    pub async fn create_comment(&self, comment: &Comment) -> Result<(), sqlx::Error> {

        self.ensure_user_exists(&comment.author_address, &comment.author_name).await?;
        
        /// Get user ID和帖子UUID
        let user_id = self.get_user_id_by_address(&comment.author_address).await?;
        let post_uuid = Uuid::parse_str(&comment.post_id)
            .map_err(|_| sqlx::Error::RowNotFound)?;
        let comment_uuid = Uuid::parse_str(&comment.id)
            .map_err(|_| sqlx::Error::RowNotFound)?;
        

        let content_hash = format!("{:x}", md5::compute(&comment.content));
        
        
        sqlx::query(
            r#"
            INSERT INTO comments (id, post_id, author_id, content, content_hash, parent_id, upvotes, created_at, updated_at, author_name, image)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#
        )
        .bind(comment_uuid)
        .bind(post_uuid)
        .bind(user_id)
        .bind(&comment.content)
        .bind(content_hash)
        .bind(comment.parent_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()))
        .bind(comment.likes as i32) 
        // .bind(comment.created_at)
        .bind(comment.created_at) 
        .bind(&comment.author_name)
        .bind(&comment.image)
        .execute(&self.pool)
        .await?;
        

        self.update_user_stats(&comment.author_address, false, true).await?;
        
        info!("📊 评论已保存到数据库: {}", comment.id);
        Ok(())
    }

    /// Get comments for a post
    pub async fn get_comments_by_post_id(&self, post_id: &str) -> Result<Vec<Comment>, sqlx::Error> {
        let post_uuid = match Uuid::parse_str(post_id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(vec![]), 
        };
        
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.post_id, c.content, c.parent_id, COALESCE(c.likes, 0) as likes,
                   c.created_at, c.irys_transaction_id, 
                   COALESCE(c.author_name, u.username) as author_name, u.avatar as author_avatar, c.image,
                   COALESCE(c.content_hash, '') as content_hash,
                   u.ethereum_address, u.id as user_id
            FROM comments c
            JOIN users u ON c.author_id = u.id
            WHERE c.post_id = $1
            ORDER BY c.created_at DESC
            "#
        )
        .bind(post_uuid)
        .fetch_all(&self.pool)
        .await?;

        let comments = rows.into_iter().map(|row| Comment {
            id: row.try_get::<Uuid, _>("id").unwrap().to_string(),
            post_id: row.try_get::<Uuid, _>("post_id").unwrap().to_string(),
            content: row.try_get("content").unwrap(),
            author_address: row.try_get::<Option<String>, _>("ethereum_address").unwrap().unwrap_or_default(),
            author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
            author_name: row.try_get("author_name").unwrap(),
            author_avatar: row.try_get("author_avatar").ok(),
            created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            parent_id: row.try_get::<Option<Uuid>, _>("parent_id").ok().flatten().map(|u| u.to_string()),
            likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
            irys_transaction_id: row.try_get("irys_transaction_id").ok(),
            image: row.try_get("image").ok(),
            content_hash: row.try_get("content_hash").unwrap_or_default(),
            is_liked_by_user: false,
        }).collect();

        Ok(comments)
    }

    /// Get comments for a post (paginated)
    pub async fn get_comments_by_post_id_paginated(&self, post_id: &str, limit: u32, offset: u32) -> Result<Vec<Comment>, sqlx::Error> {
        let post_uuid = match Uuid::parse_str(post_id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(vec![]), 
        };
        
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.post_id, c.content, c.parent_id, COALESCE(c.likes, 0) as likes,
                   c.created_at, c.irys_transaction_id, 
                   COALESCE(c.author_name, u.username) as author_name, u.avatar as author_avatar, c.image,
                   COALESCE(c.content_hash, '') as content_hash,
                   u.ethereum_address, u.id as user_id
            FROM comments c
            JOIN users u ON c.author_id = u.id
            WHERE c.post_id = $1
            ORDER BY c.created_at ASC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(post_uuid)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        let comments = rows.into_iter().map(|row| Comment {
            id: row.try_get::<Uuid, _>("id").unwrap().to_string(),
            post_id: row.try_get::<Uuid, _>("post_id").unwrap().to_string(),
            content: row.try_get("content").unwrap(),
            author_address: row.try_get::<Option<String>, _>("ethereum_address").unwrap().unwrap_or_default(),
            author_id: row.try_get::<Uuid, _>("user_id").ok().map(|id| id.to_string()),
            author_name: row.try_get("author_name").unwrap(),
            author_avatar: row.try_get("author_avatar").ok(),
            created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            parent_id: row.try_get::<Option<Uuid>, _>("parent_id").ok().flatten().map(|u| u.to_string()),
            likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
            irys_transaction_id: row.try_get("irys_transaction_id").ok(),
            image: row.try_get("image").ok(),
            content_hash: row.try_get("content_hash").unwrap_or_default(),
            is_liked_by_user: false,
        }).collect();

        Ok(comments)
    }
    
    /// Check duplicate comment within a time window in the same post
    pub async fn check_duplicate_comment(
        &self,
        author_address: &str,
        content: &str,
        post_id: &str
    ) -> Result<bool, sqlx::Error> {
        let post_uuid = uuid::Uuid::parse_str(post_id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
        

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) 
            FROM comments c
            JOIN users u ON c.author_id = u.id
            WHERE u.ethereum_address = $1 
              AND c.content = $2 
              AND c.post_id = $3
              AND c.created_at > NOW() - INTERVAL '5 minutes'
            "#
        )
        .bind(author_address)
        .bind(content)
        .bind(post_uuid)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count > 0)
    }
    
    /// Check duplicate post within a time window
    pub async fn check_duplicate_post(
        &self,
        author_address: &str,
        content: &str
    ) -> Result<bool, sqlx::Error> {

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) 
            FROM posts p
            JOIN users u ON p.author_id = u.id
            WHERE u.ethereum_address = $1 
              AND p.content = $2 
              AND p.created_at > NOW() - INTERVAL '5 minutes'
            "#
        )
        .bind(author_address)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count > 0)
    }

    /// Get user ID
    pub async fn get_user_id_by_address(&self, address: &str) -> Result<Uuid, sqlx::Error> {
        let row = sqlx::query("SELECT id FROM users WHERE ethereum_address = $1")
            .bind(address)
            .fetch_one(&self.pool)
            .await?;
        
        Ok(row.try_get("id")?)
    }

    /// Simplified user query
    pub async fn get_user_by_address(&self, address: &str) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, ethereum_address, username, bio, avatar, posts_count, comments_count, reputation, created_at FROM users WHERE LOWER(ethereum_address) = LOWER($1)"
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| User {
            id: r.try_get::<uuid::Uuid, _>("id").unwrap_or_default().to_string(),
            address: r.try_get::<Option<String>, _>("ethereum_address").unwrap_or_default().unwrap_or_default(),
            name: r.try_get("username").ok(),
            bio: r.try_get("bio").ok(),
            avatar: r.try_get("avatar").ok(),
            created_at: r.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            posts_count: r.try_get::<Option<i32>, _>("posts_count").unwrap_or(Some(0)).unwrap_or(0) as u32,
            comments_count: r.try_get::<Option<i32>, _>("comments_count").unwrap_or(Some(0)).unwrap_or(0) as u32,
            reputation: r.try_get::<Option<i32>, _>("reputation").unwrap_or(Some(0)).unwrap_or(0) as u32,
        }))
    }

    /// Ensure user exists (transactional)
    async fn ensure_user_exists_tx(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, address: &str, name: &Option<String>) -> Result<(), sqlx::Error> {

        let username = match name {
            Some(n) if !n.is_empty() => {

                let short_addr = &address[..8];
                format!("{}_{}", n, short_addr)
            },
            _ => {

                let short_addr = &address[2..10]; // 去掉0x前缀，取8位
                format!("user_{}", short_addr)
            }
        };
        
        sqlx::query(
            r#"
            INSERT INTO users (ethereum_address, username, posts_count, comments_count, reputation)
            VALUES ($1, $2, 0, 0, 0)
            ON CONFLICT (ethereum_address) DO NOTHING
            "#
        )
        .bind(address)
        .bind(&username)
        .execute(&mut **tx)
        .await?;
        
        Ok(())
    }

    /// Helper methods
    pub async fn ensure_user_exists(&self, address: &str, name: &Option<String>) -> Result<(), sqlx::Error> {

        let username = match name {
            Some(n) if !n.is_empty() => {

                let short_addr = &address[..8];
                format!("{}_{}", n, short_addr)
            },
            _ => {

                let short_addr = &address[2..10]; 
                format!("user_{}", short_addr)
            }
        };
        
        sqlx::query(
            r#"
            INSERT INTO users (ethereum_address, username, posts_count, comments_count, reputation)
            VALUES ($1, $2, 0, 0, 0)
            ON CONFLICT (ethereum_address) DO NOTHING
            "#
        )
        .bind(address)
        .bind(&username)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    async fn update_user_stats(&self, address: &str, is_post: bool, is_comment: bool) -> Result<(), sqlx::Error> {
        if is_post {

            let update_sql = format!(
                "UPDATE users SET posts_count = COALESCE(posts_count, 0) + 1, reputation = COALESCE(reputation, 0) + 10 WHERE ethereum_address = '{}'",
                address.replace("'", "''") 
            );
            sqlx::query(&update_sql)
            .execute(&self.pool)
            .await?;
        }
        
        if is_comment {

            let update_sql = format!(
                "UPDATE users SET comments_count = COALESCE(comments_count, 0) + 1, reputation = COALESCE(reputation, 0) + 5 WHERE ethereum_address = '{}'",
                address.replace("'", "''")
            );
            sqlx::query(&update_sql)
            .execute(&self.pool)
            .await?;
        }
        
        Ok(())
    }

    /// Get most active users leaderboard
    pub async fn get_active_users_ranking(&self, limit: i64) -> Result<Vec<User>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, ethereum_address, username, bio, avatar, posts_count, comments_count, reputation, created_at
            FROM users 
            WHERE posts_count > 0 OR comments_count > 0
            ORDER BY reputation DESC, posts_count DESC, comments_count DESC
            LIMIT $1
            "#
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let users = rows.into_iter().map(|r| User {
            id: r.try_get::<uuid::Uuid, _>("id").unwrap_or_default().to_string(),
            address: r.try_get::<Option<String>, _>("ethereum_address").unwrap_or_default().unwrap_or_default(),
            name: r.try_get("username").ok(),
            bio: r.try_get("bio").ok(),
            avatar: r.try_get("avatar").ok(),
            created_at: r.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            posts_count: r.try_get::<Option<i32>, _>("posts_count").unwrap_or(Some(0)).unwrap_or(0) as u32,
            comments_count: r.try_get::<Option<i32>, _>("comments_count").unwrap_or(Some(0)).unwrap_or(0) as u32,
            reputation: r.try_get::<Option<i32>, _>("reputation").unwrap_or(Some(0)).unwrap_or(0) as u32,
        }).collect();

        Ok(users)
    }

    /// Get global statistics
    pub async fn get_global_stats(&self) -> Result<GlobalStats, sqlx::Error> {
        let user_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT ethereum_address) FROM users WHERE posts_count > 0 OR comments_count > 0"
        )
        .fetch_one(&self.pool)
        .await?;

        let post_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM posts"
        )
        .fetch_one(&self.pool)
        .await?;

        let comment_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM comments"
        )
        .fetch_one(&self.pool)
        .await?;

        let total_likes = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(likes), 0) FROM posts"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(GlobalStats {
            total_users: user_count as u32,
            total_posts: post_count as u32,
            total_comments: comment_count as u32,
            total_likes: total_likes as u32,
        })
    }

    /// Like or unlike a post (toggle)
    pub async fn like_post(&self, post_id: &str, user_address: &str) -> Result<u32, sqlx::Error> {

        let post_uuid = match uuid::Uuid::parse_str(post_id) {
            Ok(uuid) => uuid,
            Err(_) => return Err(sqlx::Error::RowNotFound),
        };


        let existing_like = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM post_likes WHERE post_id = $1 AND user_address = $2"
        )
        .bind(post_uuid)
        .bind(user_address)
        .fetch_one(&self.pool)
        .await?;

        if existing_like > 0 {
  
            sqlx::query("DELETE FROM post_likes WHERE post_id = $1 AND user_address = $2")
                .bind(post_uuid)
                .bind(user_address)
                .execute(&self.pool)
                .await?;

         
            sqlx::query("UPDATE posts SET likes = GREATEST(0, likes - 1) WHERE id = $1")
                .bind(post_uuid)
                .execute(&self.pool)
                .await?;
        } else {
           
            sqlx::query(
                "INSERT INTO post_likes (post_id, user_address, created_at) VALUES ($1, $2, NOW())"
            )
            .bind(post_uuid)
            .bind(user_address)
            .execute(&self.pool)
            .await?;

          
            sqlx::query("UPDATE posts SET likes = likes + 1 WHERE id = $1")
                .bind(post_uuid)
                .execute(&self.pool)
                .await?;
        }

       
        let new_likes = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(likes, 0) FROM posts WHERE id = $1"
        )
        .bind(post_uuid)
        .fetch_one(&self.pool)
        .await?;

        Ok(new_likes as u32)
    }

    /// Check whether a user has liked a post
    pub async fn has_user_liked_post(&self, post_id: &str, user_address: &str) -> Result<bool, sqlx::Error> {
       
        let post_uuid = match uuid::Uuid::parse_str(post_id) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(false),
        };

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM post_likes WHERE post_id = $1 AND user_address = $2"
        )
        .bind(post_uuid)
        .bind(user_address)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }
    
    /// Register username (NFC-normalized to avoid confusables)
    pub async fn register_username(&self, address: &str, username: &str) -> Result<bool, sqlx::Error> {
     
        let normalized: String = username.nfc().collect::<String>().trim().to_string();
        
       
        let existing_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM users WHERE username = $1"
        )
        .bind(&normalized)
        .fetch_one(&self.pool)
        .await?;
        
        if existing_count > 0 {
            return Ok(false); 
        }
        
于数据库中（如果不存在则创建）
        self.ensure_user_exists(address, &None).await?;
        
       
        let user_has_username = sqlx::query_scalar::<_, bool>(
            "SELECT COALESCE(has_username, false) FROM users WHERE ethereum_address = $1"
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(false);
        
        if user_has_username {
            return Ok(false); 
        }
        
       
        let rows_affected = sqlx::query(
            r#"
            UPDATE users 
            SET username = $1, has_username = true, updated_at = NOW()
            WHERE ethereum_address = $2 AND (has_username = false OR has_username IS NULL)
            "#
        )
        .bind(&normalized)
        .bind(address)
        .execute(&self.pool)
        .await?
        .rows_affected();
        
        Ok(rows_affected > 0)
    }
    
    /// Check username availability (NFC-normalized)
    pub async fn is_username_available(&self, username: &str) -> Result<bool, sqlx::Error> {
       
        if !Self::is_valid_username(username) {
            return Ok(false);
        }
       
        let normalized: String = username.nfc().collect::<String>().trim().to_string();
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM users WHERE username = $1"
        )
        .bind(&normalized)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count == 0)
    }
    
    /// Get username by address
    pub async fn get_username_by_address(&self, address: &str) -> Result<Option<String>, sqlx::Error> {
        let username = sqlx::query_scalar::<_, Option<String>>(
            "SELECT username FROM users WHERE ethereum_address = $1 AND has_username = true"
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        
        Ok(username)
    }
    
    /// Check whether the user has registered a username
    pub async fn user_has_username(&self, address: &str) -> Result<bool, sqlx::Error> {
        let has_username = sqlx::query_scalar::<_, bool>(
            "SELECT COALESCE(has_username, false) FROM users WHERE ethereum_address = $1"
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(false);
        
        Ok(has_username)
    }
    
    /// Validate username format (letters/digits/underscore, 3-20 chars)
    fn is_valid_username(username: &str) -> bool {
       
        let char_count = username.chars().count();
        if char_count < 2 || char_count > 20 {
            return false;
        }
        
      
        username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '·')
    }

    /// Check whether a transaction hash has been used
    pub async fn is_transaction_used(&self, tx_hash: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM used_transactions WHERE transaction_hash = $1"
        )
        .bind(tx_hash)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(result > 0)
    }
    
    /// Record used transaction (post)
    pub async fn record_post_transaction(
        &self, 
        tx_hash: &str, 
        user_address: &str, 
        block_number: u64,
        block_timestamp: chrono::DateTime<chrono::Utc>,
        post_id: &str
    ) -> Result<(), sqlx::Error> {
        
        let post_uuid = uuid::Uuid::parse_str(post_id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
            
        sqlx::query(
            r#"
            INSERT INTO used_transactions 
            (transaction_hash, transaction_type, user_address, block_number, block_timestamp, post_id)
            VALUES ($1, 'POST', $2, $3, $4, $5)
            "#
        )
        .bind(tx_hash)
        .bind(user_address)
        .bind(block_number as i64)
        .bind(block_timestamp)
        .bind(post_uuid)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Record used transaction (comment)
    pub async fn record_comment_transaction(
        &self, 
        tx_hash: &str, 
        user_address: &str, 
        block_number: u64,
        block_timestamp: chrono::DateTime<chrono::Utc>,
        comment_id: &str
    ) -> Result<(), sqlx::Error> {
     
        let comment_uuid = uuid::Uuid::parse_str(comment_id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
            
        sqlx::query(
            r#"
            INSERT INTO used_transactions 
            (transaction_hash, transaction_type, user_address, block_number, block_timestamp, comment_id)
            VALUES ($1, 'COMMENT', $2, $3, $4, $5)
            "#
        )
        .bind(tx_hash)
        .bind(user_address)
        .bind(block_number as i64)
        .bind(block_timestamp)
        .bind(comment_uuid)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Record used transaction (username registration)
    pub async fn record_username_transaction(
        &self, 
        tx_hash: &str, 
        user_address: &str, 
        block_number: u64,
        block_timestamp: chrono::DateTime<chrono::Utc>
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO used_transactions 
            (transaction_hash, transaction_type, user_address, block_number, block_timestamp)
            VALUES ($1, 'USERNAME_REGISTER', $2, $3, $4)
            "#
        )
        .bind(tx_hash)
        .bind(user_address)
        .bind(block_number as i64)
        .bind(block_timestamp)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Update post blockchain transaction hash
    pub async fn update_post_blockchain_hash(&self, post_id: &str, tx_hash: &str) -> Result<(), sqlx::Error> {
       
        let post_uuid = uuid::Uuid::parse_str(post_id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
            
        sqlx::query(
            "UPDATE posts SET blockchain_transaction_hash = $1 WHERE id = $2"
        )
        .bind(tx_hash)
        .bind(post_uuid)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Update comment blockchain transaction hash
    pub async fn update_comment_blockchain_hash(&self, comment_id: &str, tx_hash: &str) -> Result<(), sqlx::Error> {
        
        let comment_uuid = uuid::Uuid::parse_str(comment_id)
            .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
            
        sqlx::query(
            "UPDATE comments SET blockchain_transaction_hash = $1 WHERE id = $2"
        )
        .bind(tx_hash)
        .bind(comment_uuid)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Get user's transaction records
    pub async fn get_user_transactions(&self, user_address: &str) -> Result<Vec<UserTransaction>, sqlx::Error> {
        let transactions = sqlx::query_as::<_, UserTransaction>(
            r#"
            SELECT 
                transaction_hash,
                transaction_type as transaction_type_str,
                user_address,
                block_number,
                block_timestamp,
                verified_at,
                post_id,
                comment_id
            FROM used_transactions 
            WHERE user_address = $1 
            ORDER BY verified_at DESC
            "#
        )
        .bind(user_address)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(transactions)
    }

    /// Follow system related methods
    pub async fn follow_user(&self, follower_address: &str, following_address: &str) -> Result<bool, sqlx::Error> {
      
        let existing = sqlx::query!(
            "SELECT id FROM follows WHERE follower_address = $1 AND following_address = $2",
            follower_address,
            following_address
        )
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Ok(false); 
        }


        sqlx::query!(
            "INSERT INTO follows (follower_address, following_address) VALUES ($1, $2)",
            follower_address,
            following_address
        )
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    pub async fn unfollow_user(&self, follower_address: &str, following_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM follows WHERE follower_address = $1 AND following_address = $2",
            follower_address,
            following_address
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn is_following(&self, follower_address: &str, following_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "SELECT id FROM follows WHERE follower_address = $1 AND following_address = $2",
            follower_address,
            following_address
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn get_follow_counts(&self, user_address: &str) -> Result<(u32, u32, u32), sqlx::Error> {
        
        let following_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM follows WHERE follower_address = $1",
            user_address
        )
        .fetch_one(&self.pool)
        .await?
        .count.unwrap_or(0) as u32;

        
        let followers_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM follows WHERE following_address = $1",
            user_address
        )
        .fetch_one(&self.pool)
        .await?
        .count.unwrap_or(0) as u32;

        
        let mutual_follows_count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM follows f1
            JOIN follows f2 ON f1.following_address = f2.follower_address 
                           AND f1.follower_address = f2.following_address
            WHERE f1.follower_address = $1
            "#,
            user_address
        )
        .fetch_one(&self.pool)
        .await?
        .count.unwrap_or(0) as u32;

        Ok((following_count, followers_count, mutual_follows_count))
    }

    pub async fn get_following_list(&self, user_address: &str, limit: i64, offset: i64) -> Result<Vec<crate::models::UserProfile>, sqlx::Error> {
        let users = sqlx::query!(
            r#"
            SELECT 
                u.id,
                u.username,
                u.bio,
                u.avatar,
                u.posts_count,
                u.comments_count,
                u.reputation,
                u.created_at,
                u.ethereum_address
            FROM follows f
            JOIN users u ON f.following_address = u.ethereum_address
            WHERE f.follower_address = $1
            ORDER BY f.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_address,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let mut profiles = Vec::new();
        for user in users {
            let user_id = user.id.to_string();
            let ethereum_address = user.ethereum_address.unwrap_or_default();
            let (following_count, followers_count, mutual_follows_count) = self.get_follow_counts(&ethereum_address).await.unwrap_or((0, 0, 0));
            let is_following = true; 
            let is_followed_by = self.is_following(&ethereum_address, user_address).await.unwrap_or(false);
            let is_self = ethereum_address == user_address;

            profiles.push(crate::models::UserProfile {
                id: user_id,
                ethereum_address: ethereum_address.clone(),
                username: user.username,
                bio: user.bio,
                avatar: user.avatar,
                posts_count: user.posts_count.unwrap_or(0) as u32,
                comments_count: user.comments_count.unwrap_or(0) as u32,
                reputation: user.reputation.unwrap_or(0) as u32,
                following_count,
                followers_count,
                mutual_follows_count,
                is_following,
                is_followed_by,
                is_mutual: is_following && is_followed_by,
                is_self,
                created_at: user.created_at.unwrap_or_default(),
            });
        }

        Ok(profiles)
    }

    pub async fn get_followers_list(&self, user_address: &str, limit: i64, offset: i64) -> Result<Vec<crate::models::UserProfile>, sqlx::Error> {
        let users = sqlx::query!(
            r#"
            SELECT 
                u.id,
                u.username,
                u.bio,
                u.avatar,
                u.posts_count,
                u.comments_count,
                u.reputation,
                u.created_at,
                u.ethereum_address
            FROM follows f
            JOIN users u ON f.follower_address = u.ethereum_address
            WHERE f.following_address = $1
            ORDER BY f.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_address,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let mut profiles = Vec::new();
        for user in users {
            let user_id = user.id.to_string();
            let ethereum_address = user.ethereum_address.unwrap_or_default();
            let (following_count, followers_count, mutual_follows_count) = self.get_follow_counts(&ethereum_address).await.unwrap_or((0, 0, 0));
            let is_following = self.is_following(user_address, &ethereum_address).await.unwrap_or(false);
            let is_followed_by = true; 
            let is_self = ethereum_address == user_address;

            profiles.push(crate::models::UserProfile {
                id: user_id,
                ethereum_address: ethereum_address.clone(),
                username: user.username,
                bio: user.bio,
                avatar: user.avatar,
                posts_count: user.posts_count.unwrap_or(0) as u32,
                comments_count: user.comments_count.unwrap_or(0) as u32,
                reputation: user.reputation.unwrap_or(0) as u32,
                following_count,
                followers_count,
                mutual_follows_count,
                is_following,
                is_followed_by,
                is_mutual: is_following && is_followed_by,
                is_self,
                created_at: user.created_at.unwrap_or_default(),
            });
        }

        Ok(profiles)
    }

    /// Get user address by user ID
    pub async fn get_user_address_by_id(&self, user_id: &str) -> Result<String, sqlx::Error> {
        let user_uuid = uuid::Uuid::parse_str(user_id)
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid UUID: {}", e)))?;
        
        let user = sqlx::query!("SELECT ethereum_address FROM users WHERE id = $1", user_uuid)
            .fetch_optional(&self.pool)
            .await?;
        
        if let Some(user) = user {
            Ok(user.ethereum_address.unwrap_or_default())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    pub async fn get_mutual_follows_list(&self, user_address: &str, limit: i64, offset: i64) -> Result<Vec<crate::models::UserProfile>, sqlx::Error> {
        let users = sqlx::query!(
            r#"
            SELECT 
                u.id,
                u.username,
                u.bio,
                u.avatar,
                u.posts_count,
                u.comments_count,
                u.reputation,
                u.created_at,
                u.ethereum_address
            FROM follows f1
            JOIN follows f2 ON f1.following_address = f2.follower_address 
                           AND f1.follower_address = f2.following_address
            JOIN users u ON f1.following_address = u.ethereum_address
            WHERE f1.follower_address = $1
            ORDER BY f1.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_address,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let mut profiles = Vec::new();
        for user in users {
            let user_id = user.id.to_string();
            let ethereum_address = user.ethereum_address.unwrap_or_default();
            let (following_count, followers_count, mutual_follows_count) = self.get_follow_counts(&ethereum_address).await.unwrap_or((0, 0, 0));
            let is_self = ethereum_address == user_address;

            profiles.push(crate::models::UserProfile {
                id: user_id,
                ethereum_address: ethereum_address.clone(),
                username: user.username,
                bio: user.bio,
                avatar: user.avatar,
                posts_count: user.posts_count.unwrap_or(0) as u32,
                comments_count: user.comments_count.unwrap_or(0) as u32,
                reputation: user.reputation.unwrap_or(0) as u32,
                following_count,
                followers_count,
                mutual_follows_count,
                is_following: true,  
                is_followed_by: true,
                is_mutual: true,
                is_self,
                created_at: user.created_at.unwrap_or_default(),
            });
        }

        Ok(profiles)
    }
}

/// User transaction record struct
#[derive(sqlx::FromRow, serde::Serialize, serde::Deserialize, Debug)]
pub struct UserTransaction {
    pub transaction_hash: String,
    pub transaction_type_str: String, 
    pub user_address: String,
    pub block_number: Option<i64>,
    pub block_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_at: chrono::DateTime<chrono::Utc>,
    pub post_id: Option<String>,
    pub comment_id: Option<String>,
}

impl DatabaseService {
    /// Update user avatar
    pub async fn update_user_avatar(&self, user_address: &str, avatar_url: &str) -> Result<(), sqlx::Error> {
        
        let existing_address = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT ethereum_address FROM users 
            WHERE LOWER(ethereum_address) = LOWER($1)
            LIMIT 1
            "#
        )
        .bind(user_address)
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        if let Some(addr) = existing_address {
          
            sqlx::query(
                r#"
                UPDATE users 
                SET avatar = $1, updated_at = NOW()
                WHERE ethereum_address = $2
                "#
            )
            .bind(avatar_url)
            .bind(&addr)
            .execute(&self.pool)
            .await?;
        } else {
            
            let short_addr: String = user_address
                .trim_start_matches("0x")
                .chars()
                .take(8)
                .collect();
            let default_username = format!("user_{}", short_addr);

            sqlx::query(
                r#"
                INSERT INTO users (ethereum_address, username, avatar, posts_count, comments_count, reputation, created_at, updated_at)
                VALUES ($1, $2, $3, 0, 0, 0, NOW(), NOW())
                ON CONFLICT (ethereum_address) DO UPDATE 
                  SET avatar = EXCLUDED.avatar, updated_at = NOW()
                "#
            )
            .bind(user_address)
            .bind(default_username)
            .bind(avatar_url)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Update user bio
    pub async fn update_user_bio(&self, user_address: &str, bio: &str) -> Result<(), sqlx::Error> {
       
        let existing_address = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT ethereum_address FROM users 
            WHERE LOWER(ethereum_address) = LOWER($1)
            LIMIT 1
            "#
        )
        .bind(user_address)
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        if let Some(addr) = existing_address {
         
            sqlx::query(
                r#"
                UPDATE users 
                SET bio = $1, updated_at = NOW()
                WHERE ethereum_address = $2
                "#
            )
            .bind(bio)
            .bind(&addr)
            .execute(&self.pool)
            .await?;
        } else {
           
            let short_addr: String = user_address
                .trim_start_matches("0x")
                .chars()
                .take(8)
                .collect();
            let default_username = format!("user_{}", short_addr);

            sqlx::query(
                r#"
                INSERT INTO users (ethereum_address, username, bio, posts_count, comments_count, reputation, created_at, updated_at)
                VALUES ($1, $2, $3, 0, 0, 0, NOW(), NOW())
                ON CONFLICT (ethereum_address) DO UPDATE 
                  SET bio = EXCLUDED.bio, updated_at = NOW()
                "#
            )
            .bind(user_address)
            .bind(default_username)
            .bind(bio)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Check whether daily recommendations need refresh
    pub async fn should_refresh_daily_recommendations(&self) -> Result<bool, sqlx::Error> {
       
        let today = chrono::Utc::now().date_naive();
        
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) 
            FROM daily_recommendations 
            WHERE DATE(created_at) = $1
            "#
        )
        .bind(today)
        .fetch_one(&self.pool)
        .await.unwrap_or(0);
        
        Ok(count == 0)
    }


    pub async fn calculate_hot_posts(&self) -> Result<Vec<String>, sqlx::Error> {
        let query = r#"
            WITH post_stats AS (
                SELECT 
                    p.id,
                    p.created_at,
                    COALESCE(p.likes, 0) as likes,
                    (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                    COALESCE(p.views, 0) as views,
                    -- Time decay factor calculation
                    CASE 
                        WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 24 THEN 1.0
                        WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 48 THEN 0.8
                        WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 72 THEN 0.6
                        ELSE 0.4
                    END as time_decay,
                    -- Heat score calculation: (likes x 3+comments x 2+views x 0.1) x time decay factor
                    (COALESCE(p.likes, 0) * 3 + 
                     (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) * 2 + 
                     COALESCE(p.views, 0) * 0.1) * 
                    CASE 
                        WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 24 THEN 1.0
                        WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 48 THEN 0.8
                        WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 72 THEN 0.6
                        ELSE 0.4
                    END as heat_score
                FROM posts p
                WHERE p.created_at >= NOW() - INTERVAL '7 days'  -- Only consider posts from the past 7 days
                AND p.title IS NOT NULL 
                AND p.content IS NOT NULL
            )
            SELECT id
            FROM post_stats
            WHERE heat_score > 0  -- Only select posts with popularity
            ORDER BY heat_score DESC
            LIMIT 10
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await?;

        let post_ids: Vec<String> = rows
            .into_iter()
            .map(|row| row.try_get::<uuid::Uuid, _>("id").unwrap().to_string())
            .collect();

        println!("🔥 Calculate {} popular posts", post_ids.len());
        Ok(post_ids)
    }

    /// Update daily recommendation cache
    pub async fn update_daily_recommendations(&self, post_ids: &[String]) -> Result<(), sqlx::Error> {
    
        let today = chrono::Utc::now().date_naive();
        sqlx::query("DELETE FROM daily_recommendations WHERE DATE(created_at) = $1")
            .bind(today)
            .execute(&self.pool)
            .await?;


        for (rank, post_id) in post_ids.iter().enumerate() {
            let post_uuid = uuid::Uuid::parse_str(post_id)
                .map_err(|e| sqlx::Error::TypeNotFound { type_name: format!("Invalid UUID: {}", e) })?;
            
            sqlx::query(
                r#"
                INSERT INTO daily_recommendations (post_id, rank_position, created_at)
                VALUES ($1, $2, NOW())
                "#
            )
            .bind(post_uuid)
            .bind((rank + 1) as i32)
            .execute(&self.pool)
            .await?;
        }

        println!("✅ Daily recommended cache has been updated");
        Ok(())
    }

    /// Get daily recommendations
    pub async fn get_daily_recommendations(&self, user_address: Option<&str>) -> Result<crate::models::RecommendationResult, sqlx::Error> {
        let query = if let Some(user_addr) = user_address {
            r#"
            SELECT 
                p.id, p.title, p.content, p.created_at, p.image,
                COALESCE(p.likes, 0) as likes,
                (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                COALESCE(p.views, 0) as views,
                COALESCE(p.tags, '{}') as tags,
                u.ethereum_address,
                COALESCE(u.username, p.author_name) as author_name,
                u.avatar as author_avatar,
                CASE WHEN pl.user_address IS NOT NULL THEN true ELSE false END as is_liked_by_user,
                -- Recalculate heat score for display
                (COALESCE(p.likes, 0) * 3 + 
                 (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) * 2 + 
                 COALESCE(p.views, 0) * 0.1) * 
                CASE 
                    WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 24 THEN 1.0
                    WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 48 THEN 0.8
                    WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 72 THEN 0.6
                    ELSE 0.4
                END as heat_score,
                dr.created_at as recommendation_date
            FROM daily_recommendations dr
            JOIN posts p ON dr.post_id = p.id
            JOIN users u ON p.author_id = u.id
            LEFT JOIN post_likes pl ON pl.post_id = p.id AND LOWER(pl.user_address) = LOWER($1)
            WHERE DATE(dr.created_at) = CURRENT_DATE
            ORDER BY dr.rank_position ASC
            "#
        } else {
            r#"
            SELECT 
                p.id, p.title, p.content, p.created_at, p.image,
                COALESCE(p.likes, 0) as likes,
                (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) as comments_count,
                COALESCE(p.views, 0) as views,
                COALESCE(p.tags, '{}') as tags,
                u.ethereum_address,
                COALESCE(u.username, p.author_name) as author_name,
                u.avatar as author_avatar,
                false as is_liked_by_user,
                -- Recalculate heat score for display
                (COALESCE(p.likes, 0) * 3 + 
                 (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) * 2 + 
                 COALESCE(p.views, 0) * 0.1) * 
                CASE 
                    WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 24 THEN 1.0
                    WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 48 THEN 0.8
                    WHEN EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 <= 72 THEN 0.6
                    ELSE 0.4
                END as heat_score,
                dr.created_at as recommendation_date
            FROM daily_recommendations dr
            JOIN posts p ON dr.post_id = p.id
            JOIN users u ON p.author_id = u.id
            WHERE DATE(dr.created_at) = CURRENT_DATE
            ORDER BY dr.rank_position ASC
            "#
        };

        let rows = if let Some(user_addr) = user_address {
            sqlx::query(query)
                .bind(user_addr)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(query)
                .fetch_all(&self.pool)
                .await?
        };

        let mut posts = Vec::new();
        let mut last_refresh_time = None;

        for row in rows {
            if last_refresh_time.is_none() {
                last_refresh_time = row.try_get::<chrono::DateTime<chrono::Utc>, _>("recommendation_date").ok();
            }

            let post = crate::models::Post {
                id: row.try_get::<uuid::Uuid, _>("id").unwrap().to_string(),
                title: row.try_get("title").unwrap(),
                content: row.try_get("content").unwrap(),
                author_address: row.try_get("ethereum_address").unwrap(),
                author_id: None, 
                author_name: row.try_get("author_name").ok(),
                author_avatar: row.try_get("author_avatar").ok(),
                created_at: row.try_get("created_at").unwrap(),
                updated_at: row.try_get("created_at").unwrap(), 
                likes: row.try_get::<i32, _>("likes").unwrap_or(0) as u32,
                comments_count: row.try_get::<i64, _>("comments_count").unwrap_or(0) as u32,
                views: row.try_get::<i32, _>("views").unwrap_or(0) as u32,
                tags: row.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
                irys_transaction_id: None,
                image: row.try_get("image").ok(),
                blockchain_post_id: None,
                is_liked_by_user: row.try_get::<bool, _>("is_liked_by_user").unwrap_or(false),
                heat_score: Some(row.try_get::<f64, _>("heat_score").unwrap_or(0.0)),
            };
            posts.push(post);
        }

        Ok(crate::models::RecommendationResult {
            posts,
            last_refresh_time,
        })
    }
} 