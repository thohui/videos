use std::net::SocketAddr;

use anyhow::Context;

pub struct Config {
    pub data_directory: String,
    pub database_path: String,
    pub address: SocketAddr,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        _ = dotenvy::dotenv();

        let data_directory = std::env::var("VIDEOS_DATA_DIRECTORY").unwrap_or("./data".into());
        let database_path = std::env::var("VIDEOS_DATABASE_PATH").unwrap_or("videos.db".into());

        let address: SocketAddr = std::env::var("VIDEOS_ADDR")
            .as_deref()
            .unwrap_or("127.0.0.1:3000")
            .parse()
            .context("invalid VIDEOS_ADDR")?;

        Ok(Self {
            data_directory,
            database_path,
            address,
        })
    }
}
