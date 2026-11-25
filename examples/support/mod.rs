#[cfg(windows)]
pub fn memory() -> serde_json::Value {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let mut info = PROCESS_MEMORY_COUNTERS_EX {
        cb: size,
        ..Default::default()
    };
    // The EX structure starts with PROCESS_MEMORY_COUNTERS. Windows uses cb to select its layout.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut info as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            size,
        )
    };
    assert_ne!(ok, 0, "GetProcessMemoryInfo");
    serde_json::json!({"working_set": info.WorkingSetSize, "peak_working_set": info.PeakWorkingSetSize,
           "private_bytes": info.PrivateUsage})
}

#[cfg(not(windows))]
pub fn memory() -> serde_json::Value {
    serde_json::Value::Null
}
