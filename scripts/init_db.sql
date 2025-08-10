-- Check and create necessary extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create or update user table (adapt to existing structure)
DO $$ 
BEGIN
    -- If table does not exist, create table
    IF NOT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'users') THEN
        CREATE TABLE users (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) UNIQUE NOT NULL,
            email VARCHAR(255) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            reputation INTEGER DEFAULT 0,
            irys_address VARCHAR(255),
            ethereum_address VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );
    END IF;
    
    -- Add missing columns (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'posts_count') THEN
        ALTER TABLE users ADD COLUMN posts_count INTEGER DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'comments_count') THEN
        ALTER TABLE users ADD COLUMN comments_count INTEGER DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'bio') THEN
        ALTER TABLE users ADD COLUMN bio TEXT;
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'avatar') THEN
        ALTER TABLE users ADD COLUMN avatar VARCHAR(255);
    END IF;
    
    -- Add unique constraint for ethereum_address (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.table_constraints WHERE table_name = 'users' AND constraint_name = 'users_ethereum_address_key') THEN
        ALTER TABLE users ADD CONSTRAINT users_ethereum_address_key UNIQUE (ethereum_address);
    END IF;
    
    -- Ensure posts table has likes column
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'likes') THEN
        ALTER TABLE posts ADD COLUMN likes INTEGER DEFAULT 0;
    END IF;
    
    -- Ensure posts table has tags column
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'tags') THEN
        ALTER TABLE posts ADD COLUMN tags TEXT[] DEFAULT '{}';
    END IF;
    
    -- Ensure posts table has irys_transaction_id column
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'irys_transaction_id') THEN
        ALTER TABLE posts ADD COLUMN irys_transaction_id VARCHAR(255);
    END IF;
    
    -- Add user name registration related fields
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'has_username') THEN
        ALTER TABLE users ADD COLUMN has_username BOOLEAN DEFAULT FALSE;
    END IF;
    
    -- Ensure username column allows NULL (for users without registered username)
    IF EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'username' AND is_nullable = 'NO') THEN
        ALTER TABLE users ALTER COLUMN username DROP NOT NULL;
    END IF;
    
    -- Ensure email column allows NULL (because we use ethereum address as the main identifier)
    IF EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'email' AND is_nullable = 'NO') THEN
        ALTER TABLE users ALTER COLUMN email DROP NOT NULL;
    END IF;
    
    -- Ensure password_hash column allows NULL (because we use wallet authentication)
    IF EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'password_hash' AND is_nullable = 'NO') THEN
        ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
    END IF;
    
    -- Add user statistics fields
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'posts_count') THEN
        ALTER TABLE users ADD COLUMN posts_count INTEGER DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'comments_count') THEN
        ALTER TABLE users ADD COLUMN comments_count INTEGER DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'reputation') THEN
        ALTER TABLE users ADD COLUMN reputation INTEGER DEFAULT 0;
    END IF;
END $$;

    -- Create or update posts table (adapt to existing structure)
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'posts') THEN
        CREATE TABLE posts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            title VARCHAR(255) NOT NULL,
            content TEXT NOT NULL,
            author_id UUID NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            likes INTEGER DEFAULT 0,
            comments_count INTEGER DEFAULT 0,
            FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE CASCADE
        );
    END IF;
    
    -- Add missing columns
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'tags') THEN
        ALTER TABLE posts ADD COLUMN tags TEXT[];
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'irys_transaction_id') THEN
        ALTER TABLE posts ADD COLUMN irys_transaction_id VARCHAR(255);
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'author_name') THEN
        ALTER TABLE posts ADD COLUMN author_name VARCHAR(100);
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'image') THEN
        ALTER TABLE posts ADD COLUMN image TEXT;
    END IF;
    
    -- Add blockchain transaction hash field, for preventing replay attacks
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'blockchain_transaction_hash') THEN
        ALTER TABLE posts ADD COLUMN blockchain_transaction_hash VARCHAR(66);
    END IF;
    
    -- Add smart contract post ID field
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'blockchain_post_id') THEN
        ALTER TABLE posts ADD COLUMN blockchain_post_id INTEGER;
    END IF;
    
    -- Add comment count field
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'comments_count') THEN
        ALTER TABLE posts ADD COLUMN comments_count INTEGER DEFAULT 0;
    END IF;
END $$;

-- Create or update comments table (adapt to existing structure)
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'comments') THEN
        CREATE TABLE comments (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            content TEXT NOT NULL,
            author_id UUID NOT NULL,
            post_id UUID NOT NULL,
            parent_id UUID,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            likes INTEGER DEFAULT 0,
            FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE,
            FOREIGN KEY (parent_id) REFERENCES comments(id) ON DELETE CASCADE
        );
    END IF;
    
    -- Add missing columns
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'irys_transaction_id') THEN
        ALTER TABLE comments ADD COLUMN irys_transaction_id VARCHAR(255);
    END IF;
    
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'author_name') THEN
        ALTER TABLE comments ADD COLUMN author_name VARCHAR(100);
    END IF;
    
    -- Add likes column (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'likes') THEN
        ALTER TABLE comments ADD COLUMN likes INTEGER DEFAULT 0;
    END IF;
    
    -- Add image column (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'image') THEN
        ALTER TABLE comments ADD COLUMN image TEXT;
    END IF;
    
    -- Add content_hash column (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'content_hash') THEN
        ALTER TABLE comments ADD COLUMN content_hash VARCHAR(64);
    END IF;
    
    -- Add updated_at column (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'updated_at') THEN
        ALTER TABLE comments ADD COLUMN updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW();
    END IF;
    
    -- Add blockchain transaction hash field, for preventing replay attacks
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'comments' AND column_name = 'blockchain_transaction_hash') THEN
        ALTER TABLE comments ADD COLUMN blockchain_transaction_hash VARCHAR(66);
    END IF;
END $$;

    -- Create like record table (prevent duplicate likes)
CREATE TABLE IF NOT EXISTS comment_likes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    comment_id UUID NOT NULL,
    user_address VARCHAR(42) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    FOREIGN KEY (comment_id) REFERENCES comments(id) ON DELETE CASCADE,
    UNIQUE(comment_id, user_address) -- Ensure each user can only like each comment once
);

CREATE TABLE IF NOT EXISTS post_likes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL,
    user_address VARCHAR(42) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE,
    UNIQUE(post_id, user_address) -- Ensure each user can only like each post once
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_posts_author_id ON posts(author_id);
CREATE INDEX IF NOT EXISTS idx_posts_created_at ON posts(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_comments_post_id ON comments(post_id);
CREATE INDEX IF NOT EXISTS idx_comments_author_id ON comments(author_id);
CREATE INDEX IF NOT EXISTS idx_users_ethereum_address ON users(ethereum_address);
CREATE INDEX IF NOT EXISTS idx_users_irys_address ON users(irys_address);
CREATE INDEX IF NOT EXISTS idx_comment_likes_comment_id ON comment_likes(comment_id);
CREATE INDEX IF NOT EXISTS idx_comment_likes_user_address ON comment_likes(user_address);
CREATE INDEX IF NOT EXISTS idx_post_likes_post_id ON post_likes(post_id);
CREATE INDEX IF NOT EXISTS idx_post_likes_user_address ON post_likes(user_address);

-- Create update time trigger function (if not exist)
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Add update time trigger to posts table
DROP TRIGGER IF EXISTS update_posts_updated_at ON posts;
CREATE TRIGGER update_posts_updated_at 
    BEFORE UPDATE ON posts 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

-- Add update time trigger to comments table
DROP TRIGGER IF EXISTS update_comments_updated_at ON comments;
CREATE TRIGGER update_comments_updated_at 
    BEFORE UPDATE ON comments 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

-- Create like table
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'post_likes') THEN
        CREATE TABLE post_likes (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            post_id UUID NOT NULL,
            user_address TEXT NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            UNIQUE(post_id, user_address)
        );
        
        -- Add indexes
        CREATE INDEX idx_post_likes_post_id ON post_likes(post_id);
        CREATE INDEX idx_post_likes_user_address ON post_likes(user_address);
    END IF;
END $$;

-- Transaction type enum (need to be defined first)
DO $$ BEGIN
    CREATE TYPE TRANSACTION_TYPE AS ENUM ('POST', 'COMMENT', 'USERNAME_REGISTER');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Prevent replay attacks: record used blockchain transactions
CREATE TABLE IF NOT EXISTS used_transactions (
    id SERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL UNIQUE, -- Blockchain transaction hash
    transaction_type TRANSACTION_TYPE NOT NULL, -- Transaction type enum
    user_address VARCHAR(42) NOT NULL, -- User address that initiated the transaction
    block_number BIGINT, -- Block number
    block_timestamp TIMESTAMP, -- Block timestamp
    verified_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, -- Verification time
    post_id UUID, -- Associated post ID (if it is a post transaction)
    comment_id UUID, -- Associated comment ID (if it is a comment transaction)
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE SET NULL,
    FOREIGN KEY (comment_id) REFERENCES comments(id) ON DELETE SET NULL
);

            -- Create unique index for transaction hash, to prevent reuse
CREATE UNIQUE INDEX IF NOT EXISTS idx_used_transactions_hash ON used_transactions(transaction_hash);

-- Create index for user address, for querying user's transaction records
CREATE INDEX IF NOT EXISTS idx_used_transactions_user ON used_transactions(user_address);

-- Insert example user (if not exist)
INSERT INTO users (username, email, password_hash, ethereum_address, bio) VALUES 
('anni', 'anni@example.com', 'dummy_hash', '0xb78cf3ba63a15e8dd476', 'Blockchain enthusiast')
ON CONFLICT (username) DO NOTHING; 

-- Create index for blockchain transaction hash
CREATE INDEX IF NOT EXISTS idx_posts_blockchain_tx ON posts(blockchain_transaction_hash);
CREATE INDEX IF NOT EXISTS idx_comments_blockchain_tx ON comments(blockchain_transaction_hash);

-- Create follow system table
CREATE TABLE IF NOT EXISTS follows (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    follower_address VARCHAR(42) NOT NULL,  -- Follower address
    following_address VARCHAR(42) NOT NULL, -- Following address
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(follower_address, following_address),
    CHECK(follower_address != following_address) -- Prevent self-following
);

-- Create index for follow table
CREATE INDEX IF NOT EXISTS idx_follows_follower ON follows(follower_address);
CREATE INDEX IF NOT EXISTS idx_follows_following ON follows(following_address);
CREATE INDEX IF NOT EXISTS idx_follows_created_at ON follows(created_at);

-- Daily recommendation system related tables and fields
DO $$ 
BEGIN
    -- Add views column to posts table (if not exist)
    IF NOT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'posts' AND column_name = 'views') THEN
        ALTER TABLE posts ADD COLUMN views INTEGER DEFAULT 0;
    END IF;
END $$;

            -- Create daily recommendation table

CREATE TABLE IF NOT EXISTS daily_recommendations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    rank_position INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    rec_day DATE NOT NULL DEFAULT CURRENT_DATE
);


-- Ensure daily_recommendations table exists rec_day column (compatible with existing old table)
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'daily_recommendations' 
          AND column_name = 'rec_day'
    ) THEN
        ALTER TABLE daily_recommendations ADD COLUMN rec_day DATE;
        -- Fill existing data
        UPDATE daily_recommendations 
           SET rec_day = created_at::date
         WHERE rec_day IS NULL;
        -- Set to not null
        ALTER TABLE daily_recommendations ALTER COLUMN rec_day SET NOT NULL;
    END IF;
END $$;

-- Create index for daily recommendation table to improve query performance
CREATE INDEX IF NOT EXISTS idx_daily_recommendations_rank ON daily_recommendations(rank_position);
-- Additional indexes and unique constraints, improve query and consistency
CREATE INDEX IF NOT EXISTS idx_daily_recs_created_at ON daily_recommendations (created_at);
-- Provide indexed fields for date queries and unique constraints (to avoid IMMUTABLE error)
CREATE INDEX IF NOT EXISTS idx_daily_recs_day ON daily_recommendations (rec_day);
CREATE UNIQUE INDEX IF NOT EXISTS ux_daily_recs_day_rank ON daily_recommendations (rec_day, rank_position);
CREATE INDEX IF NOT EXISTS idx_posts_views ON posts(views);

-- Maintain trigger and function for rec_day (to ensure rec_day is synchronized with created_at)
CREATE OR REPLACE FUNCTION set_daily_recs_rec_day()
RETURNS trigger AS $$
BEGIN
    NEW.rec_day := NEW.created_at::date;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_set_daily_recs_rec_day ON daily_recommendations;
CREATE TRIGGER trg_set_daily_recs_rec_day
    BEFORE INSERT OR UPDATE ON daily_recommendations
    FOR EACH ROW
    EXECUTE FUNCTION set_daily_recs_rec_day();

-- Correct rec_day for historical data (based on server local timezone)
UPDATE daily_recommendations
   SET rec_day = created_at::date
 WHERE rec_day IS DISTINCT FROM created_at::date;

-- Create view statistics function (for increasing post views)
CREATE OR REPLACE FUNCTION increment_post_views(post_uuid UUID)
RETURNS void AS $$
BEGIN
    UPDATE posts SET views = COALESCE(views, 0) + 1 WHERE id = post_uuid;
END;
$$ LANGUAGE plpgsql; 