use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct LauberhornConfig {
    pub daddr: String,
    pub port: u16,
    pub prog_num: u32,
    pub prog_ver: u32,
    pub proc_num: u32,
    pub num_cores: usize,
}

impl Default for LauberhornConfig {
    fn default() -> Self {
        LauberhornConfig {
            daddr: "10.0.0.5".into(),
            port: 12345,
            prog_num: 1, prog_ver: 1, proc_num: 1,
            num_cores: 4,
        }
    }
}

pub fn get_config() -> Result<LauberhornConfig, ()> {
    let path = std::env::var("DANDELION_LAUBERHORN_CONFIG")
    .unwrap_or_else(|_| "config.toml".to_string());

    let toml_str = std::fs::read_to_string(path)
        .map_err(|e| ())?;

        let config: LauberhornConfig = toml::from_str(&toml_str)
        .map_err(|e| ())?;
    Ok(config)
}