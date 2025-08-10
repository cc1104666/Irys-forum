use redis::{Client, Connection, Commands, RedisResult};
use crate::models::*;

pub struct CacheService {
    client: Client,
}

impl CacheService {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = Client::open(redis_url)?;
        Ok(Self { client })
    }
    
    pub fn get_connection(&self) -> RedisResult<Connection> {
        self.client.get_connection()
    }
    
   //Cache post list
    pub fn cache_posts(&self, posts: &[Post], limit: u32, offset: u32) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let key = format!("posts:{}:{}", limit, offset);
        let value = serde_json::to_string(posts).map_err(|e| {
            redis::RedisError::from((redis::ErrorKind::TypeError, "Serialization failed", e.to_string()))
        })?;
        
        conn.set_ex::<_, _, ()>(&key, value, 300)?; 
        Ok(())
    }
    
    pub fn get_cached_posts(&self, limit: u32, offset: u32) -> RedisResult<Option<Vec<Post>>> {
        let mut conn = self.get_connection()?;
        let key = format!("posts:{}:{}", limit, offset);
        
        let cached: RedisResult<String> = conn.get(&key);
        match cached {
            Ok(data) => {
                let posts: Vec<Post> = serde_json::from_str(&data).map_err(|e| {
                    redis::RedisError::from((redis::ErrorKind::TypeError, "Desialization failed", e.to_string()))
                })?;
                Ok(Some(posts))
            }
            Err(_) => Ok(None),
        }
    }
    
   //Cache comment list
    pub fn cache_comments(&self, post_id: &str, comments: &[Comment]) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let key = format!("comments:{}", post_id);
        let value = serde_json::to_string(comments).map_err(|e| {
            redis::RedisError::from((redis::ErrorKind::TypeError, "Serialization failed", e.to_string()))
        })?;
        
        conn.set_ex::<_, _, ()>(&key, value, 180)?; 
        Ok(())
    }
    
    pub fn get_cached_comments(&self, post_id: &str) -> RedisResult<Option<Vec<Comment>>> {
        let mut conn = self.get_connection()?;
        let key = format!("comments:{}", post_id);
        
        let cached: RedisResult<String> = conn.get(&key);
        match cached {
            Ok(data) => {
                let comments: Vec<Comment> = serde_json::from_str(&data).map_err(|e| {
                    redis::RedisError::from((redis::ErrorKind::TypeError, "Desialization failed", e.to_string()))
                })?;
                Ok(Some(comments))
            }
            Err(_) => Ok(None),
        }
    }
    
   
    pub fn cache_user_stats(&self, address: &str, stats: &UserStats) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let key = format!("user_stats:{}", address);
        let value = serde_json::to_string(stats).map_err(|e| {
            redis::RedisError::from((redis::ErrorKind::TypeError, "Serialization failed", e.to_string()))
        })?;
        
        conn.set_ex::<_, _, ()>(&key, value, 600)?; 
        Ok(())
    }
    
    // Cache popular users
    pub fn cache_active_users(&self, users: &[UserStats]) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let key = "active_users";
        let value = serde_json::to_string(users).map_err(|e| {
            redis::RedisError::from((redis::ErrorKind::TypeError, "Desialization failed", e.to_string()))
        })?;
        
        conn.set_ex::<_, _, ()>(&key, value, 900)?; 
        Ok(())
    }
    
   
    pub fn check_rate_limit(&self, user_address: &str, action: &str, limit: u32, window: u64) -> RedisResult<bool> {
        let mut conn = self.get_connection()?;
        let key = format!("rate_limit:{}:{}", user_address, action);
        
        let current: u32 = conn.get(&key).unwrap_or(0);
        if current >= limit {
            return Ok(false);
        }
        
        let _: () = conn.incr(&key, 1)?;
        let _: () = conn.expire(&key, window as usize)?;
        Ok(true)
    }
    
    
    pub fn acquire_lock(&self, lock_key: &str, ttl_seconds: u64) -> RedisResult<bool> {
        let mut conn = self.get_connection()?;
        
      
        let result: RedisResult<String> = redis::cmd("SET")
            .arg(&format!("lock:{}", lock_key))
            .arg("locked")
            .arg("NX")
            .arg("EX")
            .arg(ttl_seconds)
            .query(&mut conn);
        
        match result {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    pub fn release_lock(&self, lock_key: &str) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let _: () = conn.del(&format!("lock:{}", lock_key))?;
        Ok(())
    }
    
   
    pub fn invalidate_post_cache(&self) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let keys: Vec<String> = conn.keys("posts:*")?;
        if !keys.is_empty() {
            let _: () = conn.del(&keys)?;
        }
        Ok(())
    }
    
    pub fn invalidate_comment_cache(&self, post_id: &str) -> RedisResult<()> {
        let mut conn = self.get_connection()?;
        let key = format!("comments:{}", post_id);
        let _: () = conn.del(&key)?;
        Ok(())
    }
} 