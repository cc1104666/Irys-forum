# Irys Forum

A Web3 forum backend. It supports publishing, commenting, liking, user profiles, social graphs (follow/unfollow), daily recommendations, and optional integration with blockchain and Redis caching. Using interesting forms of content mining to make posts interesting.

## Key Features
- Posts and comments with optional on-chain verification (transaction hash check)
- Per-user like status for posts and comments
- User profiles, avatar upload (JPG/PNG up to 5MB), and bio updates (max 500 chars)
- Username registration and availability checks (DB and optional on-chain sync)
- Social features: follow, unfollow, followers/following/mutual lists
- Daily recommendations with periodic refresh logic
- Irys integration (mocked upload; real query endpoint)
- In-memory fallback for DB/cache/blockchain for easy local development
- Async task queue for offloading post/comment creation
- Health/performance endpoint and static asset debug endpoint

## Architecture
- HTTP server: Actix Web
- Core service: `ForumService` (business logic & integrations)
- Integrations:
  - `DatabaseService` (SQLx-based; optional)
  - `BlockchainService` (optional)
  - `CacheService` (Redis; optional)
  - `AsyncQueueService` (optional)
  - `IrysService` (HTTP client)
- Models: `Post`, `Comment`, `User`, `UserProfile`, `GlobalStats`, request/response DTOs

Graceful fallbacks when integrations are unavailable:
- No DB: in-memory storage and computed stats
- No Redis: no caching
- No blockchain: offline mode (transaction verification skipped)
- No async queue: synchronous processing used as fallback

## Tech Stack
- Language: Rust
- Web: Actix Web
- HTTP client: reqwest
- Serialization: serde / serde_json
- Database: SQLx (driver depends on `DATABASE_URL`)
- Cache: Redis (via custom `CacheService`)
- Logging: log / env_logger
- Time: chrono
- Crypto: sha2

## Environment Variables
- `DATABASE_URL`: SQLx connection string (e.g., Postgres). Optional; in-memory storage if unset/unavailable.
- `REDIS_URL`: Redis connection URL. Optional; caching disabled if unset/unavailable.
- `CONTRACT_ADDRESS`: Optional; referenced in blockchain logs.
- `RUST_LOG`: Optional; e.g., `actix_web=info,irys_forum=info`.

## Irys Integration
- Uploads are mocked by default (`IrysService::upload_data` returns `mock_tx_...`).
- Queries hit the public explorer API (configurable base URL).
- To enable real uploads, implement/uncomment the Irys RPC call inside `IrysService::upload_data` and supply proper credentials/endpoints.

## Blockchain Integration
- If the blockchain service initializes successfully, the server can verify:
  - Post transactions (`verify_post_transaction`)
  - Comment transactions (`verify_comment_transaction`)
- Handlers validate:
  - Transaction hash format: `0x`-prefixed, length 66
  - Address format: `0x`-prefixed, length 42
  - Duplicate protection windows (5 minutes) for posts and comments

## Caching
- Optional Redis caching for post lists and comments per post.
- Automatic invalidation after create/update flows.

## Async Task Queue
- Optional `AsyncQueueService` for enqueuing post/comment creation tasks.
- Handlers return a `task_id` and a status endpoint to poll.
- Falls back to synchronous creation if the queue is unavailable.

## Running Locally
1. Install a recent stable Rust toolchain
2. Set environment variables as needed:
   - `DATABASE_URL` (optional)
   - `REDIS_URL` (optional)
   - `CONTRACT_ADDRESS` (optional)
3. Build and run
   ```bash
   cargo build
   cargo run
   ```
4. By default, the server listens as configured in `main.rs` (commonly `http://localhost:8080`).

Logs will indicate whether DB, Redis, blockchain, and async queue are enabled.

## API Overview (Representative)
Note: Exact routes depend on the router setup in `main.rs`.

- Posts
  - GET posts with like status: `get_posts` (limit, offset, user_address optional)
  - GET single post with like status: `get_post` (id, user_address optional)
  - POST create post with on-chain verification: `create_post` (requires `blockchain_transaction_hash`)
  - POST create post async: `create_post_async` (returns `task_id`)

- Comments
  - POST add comment with optional on-chain verification: `add_comment`
  - GET comments for a post (paginated) with like status: `get_post_comments`
  - POST create comment async: `create_comment_async` (returns `task_id`)

- Likes
  - POST like a post: `like_post` (user_address)
  - POST like/unlike a comment (toggle): `like_comment` (user_address)

- Users
  - GET user profile by address: `get_user_profile`
  - POST avatar upload (multipart, JPG/PNG up to 5MB): `upload_avatar`
  - POST update bio: `update_bio` (max 500 chars)
  - GET a user’s own posts: `get_user_posts` (paginated; optional `user_address` to compute like status)

- Username
  - POST register username: `register_username`
  - GET check username availability: `check_username`
  - GET username by address: `get_username`
  - GET whether user has a username: `check_user_has_username`
  - POST sync username from chain to DB: `sync_user_username`

- Social Graph
  - POST follow user: `follow_user` (accepts address or id pairs)
  - POST unfollow user: `unfollow_user`
  - GET following list (paginated): `get_following_list`
  - GET followers list (paginated): `get_followers_list`
  - GET mutual follows list (paginated): `get_mutual_follows_list`
  - GET follow status (by ids or addresses): `check_follow_status`
  - GET follow stats (counts): `get_follow_stats`

- Irys
  - POST upload payload to Irys: `upload_to_irys`
  - GET query Irys with filters: `query_irys` (address, tags, limit)

- Recommendations
  - GET daily recommendations: `get_daily_recommendations` (user_address optional; returns posts and last_refresh_time)

- Tasks
  - GET task status: `get_task_status` (for the `task_id` from async endpoints)

- Monitoring/Debug
  - GET performance stats: `get_performance_stats` (DB/cache/memory snapshot)
  - GET debug static files listing: `debug_static_files` (reads `./static`)

## Validation & Constraints
- Address format: `0x`-prefixed, 42 chars
- Transaction hash: `0x`-prefixed, 66 chars
- Avatar upload: JPEG/PNG only; max 5MB
- Bio length: up to 500 characters
- Duplicate content checks (5-minute window) for posts/comments

## Development Tips
- Toggle integrations via env vars; the app logs which services are active.
- In-memory mode makes local iteration fast and easy.
- Emojis in logs help identify success (✅), warnings (⚠️), cache ops (💾), and async flows (🚀).

## Suggested Next Steps
- Add unit and integration tests
- Provide OpenAPI/Swagger documentation
- Wire up real Irys upload integration in `IrysService`
- Optional: introduce an i18n layer (e.g., via `Accept-Language`) for bilingual responses

## License
MIT (or update to your preferred license).

