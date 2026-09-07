//! 开机自启：HKCU\Software\Microsoft\Windows\CurrentVersion\Run（docs/adr/0005）。
//!
//! 注册表是唯一事实来源：设置界面的复选框读它、改它，配置文件不另存一份以免漂移。

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("EyeFlow");

pub fn is_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            Some(0),
            KEY_QUERY_VALUE,
            &mut hkey,
        )
        .is_err()
        {
            return false;
        }
        let found = RegQueryValueExW(hkey, VALUE_NAME, None, None, None, None).is_ok();
        let _ = RegCloseKey(hkey);
        found
    }
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    unsafe {
        let mut hkey = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            None,
            &mut hkey,
            None,
        )
        .ok()
        .map_err(|e| format!("打开 Run 键失败: {e}"))?;

        let result = if on {
            let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败: {e}"))?;
            let value = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(hkey, VALUE_NAME, None, REG_SZ, Some(bytes))
                .ok()
                .map_err(|e| format!("写入自启项失败: {e}"))
        } else {
            match RegDeleteValueW(hkey, VALUE_NAME).ok() {
                Ok(()) => Ok(()),
                // 值本来就不存在也算成功
                Err(e)
                    if e.code()
                        == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND.to_hresult() =>
                {
                    Ok(())
                }
                Err(e) => Err(format!("删除自启项失败: {e}")),
            }
        };
        let _ = RegCloseKey(hkey);
        result
    }
}
