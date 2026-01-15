use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct Config {
    pub home: PathBuf,
    pub addr: String,
    pub workers: usize,
}

static GLOBAL_CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    pub fn init(home: PathBuf, addr: String, workers: usize) -> Result<(), &'static str> {
        let config = Config {
            home,
            addr,
            workers,
        };
        GLOBAL_CONFIG
            .set(config)
            .map_err(|_| "Config already initialized")
    }

    pub fn get() -> &'static Config {
        GLOBAL_CONFIG
            .get()
            .expect("Config not initialized. Call Config::init() first.")
    }

    pub fn home() -> &'static PathBuf {
        &Self::get().home
    }

    pub fn addr() -> &'static String {
        &Self::get().addr
    }

    pub fn workers() -> usize {
        Self::get().workers
    }
}
