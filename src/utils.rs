use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

pub fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn format_relative_time(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);
    
    if duration.num_seconds() < 60 {
        "just".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}Minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}Hours ago", duration.num_hours())
    } else if duration.num_days() < 30 {
        format!("{}Days ago", duration.num_days())
    } else {
        format_timestamp(timestamp)
    }
}

pub fn truncate_text(text: &str, max_length: usize) -> String {
    if text.len() <= max_length {
        text.to_string()
    } else {
        format!("{}...", &text[..max_length])
    }
}

pub fn extract_tags_from_content(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let words: Vec<&str> = content.split_whitespace().collect();
    
    for word in words {
        if word.starts_with('#') && word.len() > 1 {
            let tag = word[1..].to_lowercase();
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    
    tags
}

pub fn validate_address(address: &str) -> bool {
   
    address.len() >= 26 && address.len() <= 35 && address.starts_with("1")
}

pub fn sanitize_html(input: &str) -> String {
  
    input
        .replace("<script>", "")
        .replace("</script>", "")
        .replace("<iframe>", "")
        .replace("</iframe>", "")
        .replace("javascript:", "")
        .replace("onclick", "")
        .replace("onload", "")
        .replace("onerror", "")
}

pub fn parse_irys_transaction(transaction: &Value) -> Option<HashMap<String, String>> {
    if let Some(data) = transaction.get("data") {
        if let Some(data_str) = data.as_str() {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(data_str) {
                return Some(parsed);
            }
        }
    }
    None
}

pub fn generate_avatar_url(address: &str) -> String {
 
    format!("https://www.gravatar.com/avatar/{:x}?d=identicon&s=64", 
        md5::compute(address.to_lowercase()))
}

pub fn calculate_reputation(posts_count: u32, comments_count: u32, likes_received: u32) -> u32 {
    posts_count * 10 + comments_count * 5 + likes_received * 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_format_relative_time() {
        let now = Utc::now();
        let one_minute_ago = now - Duration::minutes(1);
        let one_hour_ago = now - Duration::hours(1);
        let one_day_ago = now - Duration::days(1);
        
        assert!(format_relative_time(one_minute_ago).contains("Minutes ago"));
        assert!(format_relative_time(one_hour_ago).contains("Hours ago"));
        assert!(format_relative_time(one_day_ago).contains("Days ago"));
    }

    #[test]
    fn test_extract_tags() {
        let content = "This is a post about # rust # blockchain #irys";
        let tags = extract_tags_from_content(content);
        assert_eq!(tags, vec!["rust", "blockchain", "irys"]);
    }

    #[test]
    fn test_truncate_text() {
        let text = "This is a very long text content";
        let truncated = truncate_text(text, 10);
        assert_eq!(truncated, "This is a very long one...");
    }
} 