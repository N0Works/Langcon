use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

const WM_IME_CONTROL: u32 = 0x0283;
const IMC_GETCONVERSIONMODE_PARAM: usize = 0x0005;
const VK_HANGUL: u16 = 0x15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImeStatus {
    #[serde(rename = "english")]
    English,
    #[serde(rename = "korean")]
    Original,
    #[serde(rename = "unknown")]
    Unknown,
}

pub fn ime_status(hwnd: HWND) -> Result<ImeStatus> {
    if hwnd.0.is_null() {
        return Ok(ImeStatus::Unknown);
    }

    let ime_hwnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
    if ime_hwnd.0.is_null() {
        return Ok(ImeStatus::Unknown);
    }

    let result = unsafe {
        SendMessageW(
            ime_hwnd,
            WM_IME_CONTROL,
            WPARAM(IMC_GETCONVERSIONMODE_PARAM),
            LPARAM(0),
        )
    };

    if result == LRESULT(0) {
        Ok(ImeStatus::English)
    } else {
        Ok(ImeStatus::Original)
    }
}

pub fn ensure_english(hwnd: HWND) -> Result<bool> {
    if hwnd.0.is_null() {
        return Ok(false);
    }

    let mut toggled = false;

    for _ in 0..3 {
        match ime_status(hwnd)? {
            ImeStatus::English => return Ok(toggled),
            ImeStatus::Original | ImeStatus::Unknown => {
                toggle_hangul_key().context("IME 토글 시뮬레이션 실패")?;
                toggled = true;
                thread::sleep(Duration::from_millis(80));

                match ime_status(hwnd)? {
                    ImeStatus::English => return Ok(true),
                    ImeStatus::Original | ImeStatus::Unknown => continue,
                }
            }
        }
    }

    match ime_status(hwnd)? {
        ImeStatus::English => Ok(true),
        _ if toggled => Err(anyhow!("IME 토글 후에도 영문 전환 확인에 실패했습니다.")),
        _ => Ok(false),
    }
}

pub fn toggle_hangul_key() -> Result<()> {
    unsafe {
        let press = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_HANGUL),
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let release = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_HANGUL),
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [press, release];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            return Err(anyhow!("SendInput 실패: {}", sent));
        }
    }
    Ok(())
}
