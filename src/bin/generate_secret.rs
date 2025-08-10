use rand::Rng;
use sha2::{Digest, Sha256};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let length = if args.len() > 1 {
        args[1].parse::<usize>().unwrap_or(64)
    } else {
        64
    };

    // 生成随机字节
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..length).map(|_| rng.gen()).collect();

    // 使用SHA256生成最终的密钥
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();

    // 转换为十六进制字符串
    let secret_key = hex::encode(result);

    println!("Generated SECRET_KEY:");
    println!("SECRET_KEY={}", secret_key);
    println!();
    println!("Copy this line to your .env file");
} 