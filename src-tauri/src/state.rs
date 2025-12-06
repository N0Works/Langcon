use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::Map;

use crate::config::{AppConfig, AppConfigDto, ConfigManager, sanitize_language};
use crate::ime::ImeStatus;
use crate::process::ProcessInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusMessage {
    pub key: String,
    #[serde(default)]
    pub values: Map<String, serde_json::Value>,
}

impl StatusMessage {
    pub fn with_values(
        key: impl Into<String>,
        values: impl IntoIterator<Item = (impl Into<String>, impl Into<serde_json::Value>)>,
    ) -> Self {
        let mut map = Map::new();
        for (k, v) in values {
            map.insert(k.into(), v.into());
        }
        Self {
            key: key.into(),
            values: map,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSnapshot {
    pub process: Option<ProcessInfo>,
    pub ime_status: ImeStatus,
    pub manual_override: bool,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppViewModel {
    pub saved_config: AppConfigDto,
    pub draft_config: AppConfigDto,
    pub available_processes: Vec<ProcessInfo>,
    pub focus: Option<FocusSnapshot>,
    pub has_unsaved_changes: bool,
    pub status_message: Option<StatusMessage>,
}

#[derive(Debug, Clone)]
pub struct FocusSnapshotInternal {
    pub process: Option<ProcessInfo>,
    pub ime_status: ImeStatus,
    pub manual_override: bool,
    pub updated_at: DateTime<Local>,
}

#[derive(Debug)]
pub struct AppState {
    config_manager: std::sync::Arc<ConfigManager>,
    saved_config: AppConfig,
    draft_config: AppConfig,
    pub available_processes: Vec<ProcessInfo>,
    pub focus: Option<FocusSnapshotInternal>,
    manual_overrides: HashSet<String>,
    pub pending_process_refresh: bool,
    pub last_cursor_pos: Option<(i32, i32)>,
    pub last_status_message: Option<StatusMessage>,
    last_status_record: Option<StatusRecord>,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct StatusRecord {
    message: StatusMessage,
    at: DateTime<Local>,
}

impl AppState {
    pub fn new(config_manager: std::sync::Arc<ConfigManager>, config: AppConfig) -> Self {
        Self {
            config_manager,
            saved_config: config.clone(),
            draft_config: config,
            available_processes: Vec::new(),
            focus: None,
            manual_overrides: HashSet::new(),
            pending_process_refresh: true,
            last_cursor_pos: None,
            last_status_message: None,
            last_status_record: None,
            dirty: false,
        }
    }

    pub fn active_config(&self) -> &AppConfig {
        &self.saved_config
    }

    pub fn current_language(&self) -> &str {
        &self.draft_config.language
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.dirty
    }

    pub fn save_changes(&mut self) -> Result<bool> {
        if !self.dirty {
            return Ok(false);
        }

        let mut cfg = self.draft_config.clone();
        cfg.normalize();
        self.config_manager.save(&cfg)?;
        self.saved_config = cfg.clone();
        self.draft_config = cfg;
        self.manual_overrides
            .retain(|name| self.saved_config.selected_processes.contains(name));
        self.dirty = false;
        Ok(true)
    }

    pub fn discard_changes(&mut self) -> bool {
        if self.dirty {
            self.draft_config = self.saved_config.clone();
            self.dirty = false;
            true
        } else {
            false
        }
    }

    pub fn set_use_auto_to_en(&mut self, enabled: bool) -> Result<()> {
        if self.draft_config.use_auto_to_en != enabled {
            self.draft_config.use_auto_to_en = enabled;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn set_start_with_windows(&mut self, enabled: bool) -> Result<()> {
        if self.draft_config.start_with_windows != enabled {
            crate::startup::set_autostart(enabled)?;
            self.draft_config.start_with_windows = enabled;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn set_use_mouse_move_event(&mut self, enabled: bool) -> Result<()> {
        if self.draft_config.use_mouse_move_event != enabled {
            self.draft_config.use_mouse_move_event = enabled;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn set_detect_interval(&mut self, seconds: f32) -> Result<()> {
        let new_value = seconds.max(0.1);
        if (self.draft_config.detect_interval_secs - new_value).abs() > f32::EPSILON {
            self.draft_config.detect_interval_secs = new_value;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn set_mouse_sensitivity(&mut self, distance: f32) -> Result<()> {
        let new_value = distance.max(1.0);
        if (self.draft_config.mouse_sensitivity - new_value).abs() > f32::EPSILON {
            self.draft_config.mouse_sensitivity = new_value;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn add_selected_process(&mut self, name: impl Into<String>) -> Result<bool> {
        let process = name.into();
        if !self.draft_config.selected_processes.contains(&process) {
            self.draft_config.selected_processes.push(process);
            self.draft_config.selected_processes.sort();
            self.draft_config.selected_processes.dedup();
            self.dirty = true;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn remove_selected_process(&mut self, name: &str) -> Result<bool> {
        let len_before = self.draft_config.selected_processes.len();
        self.draft_config.selected_processes.retain(|p| p != name);
        let removed = self.draft_config.selected_processes.len() != len_before;
        if removed {
            self.dirty = true;
        }
        self.manual_overrides.remove(name);
        Ok(removed)
    }

    pub fn set_language(&mut self, language: impl AsRef<str>) -> Result<bool> {
        let normalized = sanitize_language(language);
        if self.draft_config.language != normalized {
            self.draft_config.language = normalized;
            self.dirty = true;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn set_available_processes(&mut self, processes: Vec<ProcessInfo>) {
        self.available_processes = processes;
    }

    pub fn set_focus(&mut self, focus: Option<FocusSnapshotInternal>) {
        self.focus = focus;
    }

    pub fn set_manual_override(&mut self, process_name: &str, enabled: bool) {
        if enabled {
            self.manual_overrides.insert(process_name.to_string());
        } else {
            self.manual_overrides.remove(process_name);
        }
        if let Some(snapshot) = self.focus.as_mut() {
            if let Some(proc) = &snapshot.process {
                if proc.name == process_name {
                    snapshot.manual_override = enabled;
                }
            }
        }
    }

    pub fn manual_override_for(&self, process_name: &str) -> bool {
        self.manual_overrides.contains(process_name)
    }

    pub fn request_process_refresh(&mut self) {
        self.pending_process_refresh = true;
    }

    pub fn take_process_refresh_request(&mut self) -> bool {
        if self.pending_process_refresh {
            self.pending_process_refresh = false;
            true
        } else {
            false
        }
    }

    pub fn update_status_message(&mut self, message: Option<StatusMessage>) {
        self.last_status_message = message;
    }

    pub fn should_emit_status(&self, message: &StatusMessage, cooldown_ms: i64) -> bool {
        if let Some(record) = &self.last_status_record {
            if record.message.key == message.key && record.message.values == message.values {
                let elapsed = (Local::now() - record.at).num_milliseconds();
                return elapsed >= cooldown_ms;
            }
        }
        true
    }

    pub fn record_status(&mut self, message: StatusMessage) {
        self.last_status_record = Some(StatusRecord {
            message,
            at: Local::now(),
        });
    }

    pub fn to_view_model(&mut self) -> AppViewModel {
        AppViewModel {
            saved_config: AppConfigDto::from(&self.saved_config),
            draft_config: AppConfigDto::from(&self.draft_config),
            available_processes: self.available_processes.clone(),
            focus: self.focus.as_ref().map(|f| FocusSnapshot {
                process: f.process.clone(),
                ime_status: f.ime_status,
                manual_override: f.manual_override,
                updated_at: Some(f.updated_at.format("%H:%M:%S").to_string()),
            }),
            has_unsaved_changes: self.has_unsaved_changes(),
            status_message: None,
        }
    }
}

pub type SharedAppState = std::sync::Arc<parking_lot::Mutex<AppState>>;
