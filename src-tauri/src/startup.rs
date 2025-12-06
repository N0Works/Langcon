use anyhow::{Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "Langcon";
pub const AUTOSTART_FLAG: &str = "--autostart";

/// Enable or disable auto-start via the Windows Run registry key.
pub fn set_autostart(enabled: bool) -> Result<()> {
    let exe_path = std::env::current_exe()
        .context("실행 파일 경로를 확인할 수 없습니다")?
        .display()
        .to_string();
    let command = format!("\"{exe_path}\" {AUTOSTART_FLAG}");

    unsafe {
        let mut key = HKEY::default();
        let subkey = to_wide(RUN_SUBKEY);
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()
        .context("시작 프로그램 레지스트리 키를 열 수 없습니다")?;

        let value_name = to_wide(VALUE_NAME);

        if enabled {
            let data = to_wide_with_null(&command);
            let bytes: Vec<u8> = data.iter().flat_map(|w| w.to_le_bytes()).collect();
            RegSetValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                0,
                REG_SZ,
                Some(&bytes),
            )
            .ok()
            .context("시작 프로그램 등록에 실패했습니다")?;
        } else {
            let _ = RegDeleteValueW(key, PCWSTR(value_name.as_ptr()));
        }

        let close_status = RegCloseKey(key);
        if close_status != ERROR_SUCCESS {
            anyhow::bail!("레지스트리 키를 닫는 데 실패했습니다: {close_status:?}");
        }
    }

    Ok(())
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn to_wide_with_null(value: &str) -> Vec<u16> {
    to_wide(value)
}
