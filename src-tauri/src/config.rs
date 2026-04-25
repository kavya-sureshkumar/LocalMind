use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| dirs::home_dir().unwrap().join(".localmind"));
    let dir = base.join("LocalMind");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn models_dir() -> PathBuf {
    let dir = app_dir().join("models");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn bin_dir() -> PathBuf {
    let dir = app_dir().join("bin");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn data_dir() -> PathBuf {
    let dir = app_dir().join("data");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn llama_server_path() -> PathBuf {
    let name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    bin_dir().join(name)
}

pub fn sd_binary_path() -> PathBuf {
    let name = if cfg!(windows) { "sd.exe" } else { "sd" };
    bin_dir().join(name)
}

pub fn sd_output_dir() -> PathBuf {
    let dir = data_dir().join("sd_out");
    std::fs::create_dir_all(&dir).ok();
    dir
}
