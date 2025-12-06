use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

const CONFIG_FILE_NAME: &str = "config.json";
const WINDOW_STATE_FILE_NAME: &str = "window.json";
pub const FALLBACK_LANGUAGE: &str = "en";
pub const SUPPORTED_LANGUAGES: [&str; 4] = ["en", "ko", "ja", "zh"];

fn default_language() -> String {
    FALLBACK_LANGUAGE.to_string()
}

pub fn sanitize_language(value: impl AsRef<str>) -> String {
    let lower = value.as_ref().to_lowercase();
    if SUPPORTED_LANGUAGES.iter().any(|lang| *lang == lower) {
        lower
    } else {
        FALLBACK_LANGUAGE.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(alias = "selected_process_list")]
    pub selected_processes: Vec<String>,
    #[serde(alias = "use_auto_org_to_en")]
    pub use_auto_to_en: bool,
    #[serde(alias = "use_mouse_move_event")]
    pub use_mouse_move_event: bool,
    #[serde(alias = "detect_process_language_interval")]
    pub detect_interval_secs: f32,
    #[serde(alias = "detect_mose_movement_sensitivity")]
    pub mouse_sensitivity: f32,
    #[serde(alias = "in_english")]
    pub in_english: bool,
    #[serde(alias = "start_with_windows")]
    pub start_with_windows: bool,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_processes: Vec::new(),
            use_auto_to_en: true,
            use_mouse_move_event: true,
            detect_interval_secs: 0.5,
            mouse_sensitivity: 100.0,
            in_english: false,
            start_with_windows: false,
            language: default_language(),
        }
    }
}

impl AppConfig {
    pub fn normalize(&mut self) {
        if self.detect_interval_secs <= 0.0 {
            self.detect_interval_secs = 0.5;
        }
        if self.mouse_sensitivity <= 0.0 {
            self.mouse_sensitivity = 100.0;
        }
        self.language = sanitize_language(&self.language);
        self.selected_processes.sort();
        self.selected_processes.dedup();
    }
}

#[derive(Debug, Clone)]
pub struct ConfigManager {
    config_path: PathBuf,
    window_state_path: PathBuf,
}

impl ConfigManager {
    pub fn load_or_create() -> Result<(Self, AppConfig)> {
        let dir = determine_config_dir()?;
        fs::create_dir_all(&dir).context("config 디렉터리 생성 실패")?;
        let config_path = dir.join(CONFIG_FILE_NAME);
        let window_state_path = dir.join(WINDOW_STATE_FILE_NAME);

        let mut config = if config_path.exists() {
            let raw = fs::read_to_string(&config_path).context("config 파일을 읽을 수 없습니다")?;
            match serde_json::from_str::<AppConfig>(&raw) {
                Ok(mut cfg) => {
                    cfg.normalize();
                    cfg
                }
                Err(err) => {
                    tracing::warn!(?err, path = %config_path.display(), "config 파싱 실패, 기본값 사용");
                    AppConfig::default()
                }
            }
        } else {
            AppConfig::default()
        };

        config.normalize();
        Ok((
            Self {
                config_path,
                window_state_path,
            },
            config,
        ))
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let mut cfg = config.clone();
        cfg.normalize();
        let serialized = serde_json::to_string_pretty(&cfg).context("config 직렬화 실패")?;
        fs::write(&self.config_path, serialized).context("config 저장 실패")?;
        Ok(())
    }

    pub fn load_window_state(&self) -> Result<Option<WindowState>> {
        if !self.window_state_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&self.window_state_path).context("window state 파일을 읽을 수 없습니다")?;
        match serde_json::from_str::<WindowState>(&raw) {
            Ok(state) => Ok(Some(state)),
            Err(err) => {
                tracing::warn!(?err, path = %self.window_state_path.display(), "window state 파싱 실패, 무시합니다");
                Ok(None)
            }
        }
    }

    pub fn save_window_state(&self, state: &WindowState) -> Result<()> {
        let serialized = serde_json::to_string_pretty(state).context("window state 직렬화 실패")?;
        fs::write(&self.window_state_path, serialized).context("window state 저장 실패")?;
        Ok(())
    }
}

fn determine_config_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join("N0Works").join("Langcon"));
    }

    if let Some(path) = directories::BaseDirs::new().map(|dirs| dirs.data_local_dir().to_path_buf()) {
        return Ok(path.join("N0Works").join("Langcon"));
    }

    Err(anyhow!("LOCALAPPDATA 환경 변수를 찾을 수 없습니다."))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigDto {
    pub selected_processes: Vec<String>,
    pub use_auto_to_en: bool,
    pub use_mouse_move_event: bool,
    pub detect_interval_secs: f32,
    pub mouse_sensitivity: f32,
    pub start_with_windows: bool,
    pub language: String,
}

impl From<&AppConfig> for AppConfigDto {
    fn from(value: &AppConfig) -> Self {
        Self {
            selected_processes: value.selected_processes.clone(),
            use_auto_to_en: value.use_auto_to_en,
            use_mouse_move_event: value.use_mouse_move_event,
            detect_interval_secs: value.detect_interval_secs,
            mouse_sensitivity: value.mouse_sensitivity,
            start_with_windows: value.start_with_windows,
            language: sanitize_language(&value.language),
        }
    }
}
