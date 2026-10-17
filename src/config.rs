pub struct Config {
    pub data_directory: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        _ = dotenvy::dotenv();

        let data_directory = std::env::var("DATA_DIRECTORY").unwrap_or("./data".into());

        Ok(Self { data_directory })
    }
}
