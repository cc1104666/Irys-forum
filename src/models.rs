use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Post {
    pub id: String,
    pub title: String,
    pub content: String,
    pub author_address: String,
    pub author_id: Option<String>,
    pub author_name: Option<String>,
    pub author_avatar: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub likes: u32,
    pub comments_count: u32,
    pub tags: Vec<String>,
    pub irys_transaction_id: Option<String>,
    pub image: Option<String>,
    pub blockchain_post_id: Option<u32>,
    #[serde(default)]
    pub is_liked_by_user: bool,
    #[serde(default)]
    pub views: u32,
    pub heat_score: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Comment {
    pub id: String,
    pub post_id: String,
    pub content: String,
    pub author_address: String,
    pub author_id: Option<String>,
    pub author_name: Option<String>,
    pub author_avatar: Option<String>,
    pub created_at: DateTime<Utc>,
    pub parent_id: Option<String>,
    pub likes: u32,
    pub irys_transaction_id: Option<String>,
    pub image: Option<String>,
    pub content_hash: String,
    #[serde(default)]
    pub is_liked_by_user: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub address: String,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
    pub posts_count: u32,
    pub comments_count: u32,
    pub reputation: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePostRequest {
    pub title: String,
    pub content: String,
    pub author_address: String,
    pub author_name: Option<String>,
    pub tags: Vec<String>,
    pub image: Option<String>,
    pub blockchain_transaction_hash: Option<String>,
    pub blockchain_transaction_proof: Option<String>,
    #[serde(default, deserialize_with = "de_opt_u32")]
    pub blockchain_post_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCommentRequest {
    pub post_id: String,
    pub content: String,
    pub author_address: String,
    pub author_name: Option<String>,
    pub parent_id: Option<String>,
    pub image: Option<String>,
    pub blockchain_transaction_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IrysUploadRequest {
    pub data: String,
    pub tags: Vec<String>,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IrysQueryRequest {
    pub address: Option<String>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalStats {
    pub total_users: u32,
    pub total_posts: u32,
    pub total_comments: u32,
    pub total_likes: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LikeRequest {
    pub user_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterUsernameRequest {
    pub username: String,
    pub user_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckUsernameRequest {
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncUsernameRequest {
    pub user_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsernameCheckResponse {
    pub available: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            message: None,
            error: Some(message),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserStats {
    pub ethereum_address: String,
    pub username: Option<String>,
    pub posts_count: u32,
    pub comments_count: u32,
    pub reputation: u32,
    pub following_count: u32,
    pub followers_count: u32,
    pub mutual_follows_count: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Follow {
    pub id: String,
    pub follower_address: String,
    pub following_address: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FollowRequest {
    pub follower_id: Option<String>,
    pub following_id: Option<String>,
    pub follower_address: Option<String>,
    pub following_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FollowResponse {
    pub success: bool,
    pub is_following: bool,
    pub following_count: u32,
    pub followers_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    pub id: String,
    pub ethereum_address: String,
    pub username: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    pub posts_count: u32,
    pub comments_count: u32,
    pub reputation: u32,
    pub following_count: u32,
    pub followers_count: u32,
    pub mutual_follows_count: u32,
    pub is_following: bool,
    pub is_followed_by: bool,
    pub is_mutual: bool,
    pub is_self: bool,
    pub created_at: DateTime<Utc>,
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecommendationResult {
    pub posts: Vec<Post>,
    pub last_refresh_time: Option<chrono::DateTime<chrono::Utc>>,
}

// Custom deserializer: accept number or numeric string (or null) for Option<u32>
fn de_opt_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => {
            n.as_u64()
                .and_then(|v| if v <= u32::MAX as u64 { Some(v as u32) } else { None })
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom("invalid u32 number"))
        }
        Some(serde_json::Value::String(s)) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u32>()
                    .map(Some)
                    .map_err(|_| serde::de::Error::custom("invalid u32 string"))
            }
        }
        Some(_) => Err(serde::de::Error::custom("expected number or string or null")),
    }
}
