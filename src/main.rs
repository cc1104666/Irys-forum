use actix_cors::Cors;
use actix_files::Files;
use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use log::info;
use std::time::Duration;

mod blockchain;
mod database;
mod handlers;
mod models;
mod services;
mod utils;
mod cache;
mod async_queue;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    info!("Starting Irys Forum Server...");

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_address = format!("0.0.0.0:{}", port);

    info!("Server running on http://{}", bind_address);

    // Initialize ForumService
    let forum_service = web::Data::new(std::sync::Arc::new(services::ForumService::new().await));

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(forum_service.clone())
          
            .app_data(web::JsonConfig::default().limit(10 * 1024 * 1024))
            .service(
                web::scope("/api")
                    .route("/posts", web::get().to(handlers::get_posts))
                    .route("/posts", web::post().to(handlers::create_post))
                    .route("/posts/{id}", web::get().to(handlers::get_post))
                    .route("/posts/{id}/like", web::post().to(handlers::like_post))
                    .route("/posts/{id}/comments", web::get().to(handlers::get_post_comments))
                    .route("/posts/{id}/comments", web::post().to(handlers::add_comment))
                    .route("/users/{address}", web::get().to(handlers::get_user_profile))
                    .route("/users/{address}/username", web::get().to(handlers::get_username))
                    .route("/users/{address}/has-username", web::get().to(handlers::check_user_has_username))
                    .route("/username/register", web::post().to(handlers::register_username))
                    .route("/username/check", web::get().to(handlers::check_username))
                    .route("/username/sync", web::post().to(handlers::sync_user_username))
                    .route("/stats/global", web::get().to(handlers::get_global_stats))
                    .route("/stats/active-users", web::get().to(handlers::get_active_users_ranking))
                    .route("/irys/upload", web::post().to(handlers::upload_to_irys))
                    .route("/irys/query", web::get().to(handlers::query_irys))
                    .route("/debug/static", web::get().to(handlers::debug_static_files))
                    .route("/performance", web::get().to(handlers::get_performance_stats))
                    .route("/posts/async", web::post().to(handlers::create_post_async))
                    .route("/comments/async", web::post().to(handlers::create_comment_async))
                    .route("/tasks/{task_id}", web::get().to(handlers::get_task_status))
                    .route("/users/{address}/posts", web::get().to(handlers::get_user_posts))
                    .route("/comments/{comment_id}/like", web::post().to(handlers::like_comment))
                    
                    .route("/follow", web::post().to(handlers::follow_user))
                    .route("/unfollow", web::post().to(handlers::unfollow_user))
                    .route("/users/{address}/following", web::get().to(handlers::get_following_list))
                    .route("/users/{address}/followers", web::get().to(handlers::get_followers_list))
                    .route("/users/{address}/friends", web::get().to(handlers::get_mutual_follows_list))
                    .route("/follow/status", web::get().to(handlers::check_follow_status))
                    .route("/users/{address}/follow-stats", web::get().to(handlers::get_follow_stats))
                    
                    .route("/users/avatar/upload", web::post().to(handlers::upload_avatar))
                    .route("/users/bio/update", web::post().to(handlers::update_bio))
                    
                    .route("/recommendations/daily", web::get().to(handlers::get_daily_recommendations))
            )
            .service(Files::new("/icon", "./icon"))
            .service(Files::new("/avatars", "./static/avatars"))
            .service(Files::new("/", "./static").index_file("index.html"))
    })
    .workers(num_cpus::get().max(4))
    .max_connections(2000) 
    .max_connection_rate(256)
    .client_request_timeout(Duration::from_secs(30))
    .client_disconnect_timeout(Duration::from_secs(5)) 
    .bind(&bind_address)?
    .run()
    .await
} 