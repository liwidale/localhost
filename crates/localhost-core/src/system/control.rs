#[cfg(not(windows))]
use std::path::Path;

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ControlError {
    Gone,
    Denied,
    Unsupported,
    Failed,
}

pub type ControlResult = Result<(), ControlError>;

#[cfg(unix)]
fn check(ret: i32) -> ControlResult {
    if ret == 0 {
        return Ok(());
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Err(ControlError::Gone),
        Some(libc::EPERM) => Err(ControlError::Denied),
        _ => Err(ControlError::Failed),
    }
}

#[cfg(unix)]
pub fn interrupt(pid: u32, _helper: Option<&Path>) -> ControlResult {
    let pid = pid as libc::pid_t;
    let group = unsafe { libc::getpgid(pid) };
    if group == pid {
        check(unsafe { libc::killpg(group, libc::SIGINT) })
    } else {
        check(unsafe { libc::kill(pid, libc::SIGTERM) })
    }
}

#[cfg(unix)]
pub fn kill(pid: u32) -> ControlResult {
    check(unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) })
}

#[cfg(unix)]
pub fn interrupt_group(leader: u32) -> ControlResult {
    check(unsafe { libc::killpg(leader as libc::pid_t, libc::SIGINT) })
}

#[cfg(unix)]
pub fn kill_group(leader: u32) -> ControlResult {
    check(unsafe { libc::killpg(leader as libc::pid_t, libc::SIGKILL) })
}

#[cfg(windows)]
pub use windows::{Job, ctrl_c_helper, enable_ctrl_c, interrupt, kill};

#[cfg(windows)]
mod windows {
    use super::{ControlError, ControlResult};
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, GetLastError, HANDLE,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, CTRL_C_EVENT, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, OpenProcess, PROCESS_TERMINATE, TerminateProcess,
    };

    pub const HELPER_FLAG: &str = "--localhost-ctrl-c";

    fn last_error() -> ControlError {
        match unsafe { GetLastError() } {
            ERROR_ACCESS_DENIED => ControlError::Denied,
            ERROR_INVALID_PARAMETER => ControlError::Gone,
            _ => ControlError::Failed,
        }
    }

    pub fn kill(pid: u32) -> ControlResult {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle.is_null() {
                return Err(last_error());
            }
            let ok = TerminateProcess(handle, 1);
            let err = (ok == 0).then(last_error);
            CloseHandle(handle);
            err.map_or(Ok(()), Err)
        }
    }

    pub fn interrupt(pid: u32, helper: Option<&Path>) -> ControlResult {
        let helper = helper.ok_or(ControlError::Unsupported)?;
        let mut child = std::process::Command::new(helper)
            .arg(HELPER_FLAG)
            .arg(pid.to_string())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|_| ControlError::Failed)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return match status.code() {
                        Some(0) => Ok(()),
                        Some(2) => Err(ControlError::Unsupported),
                        _ => Err(ControlError::Failed),
                    };
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    let _ = child.kill();
                    return Err(ControlError::Failed);
                }
            }
        }
    }

    pub fn ctrl_c_helper(args: &[String]) -> Option<i32> {
        let pos = args.iter().position(|a| a == HELPER_FLAG)?;
        let pid: u32 = args.get(pos + 1)?.parse().ok()?;
        unsafe {
            FreeConsole();
            if AttachConsole(pid) == 0 {
                return Some(2);
            }
            SetConsoleCtrlHandler(None, 1);
            let ok = GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0);
            FreeConsole();
            Some(if ok != 0 { 0 } else { 1 })
        }
    }

    pub fn enable_ctrl_c() {
        unsafe {
            SetConsoleCtrlHandler(None, 0);
        }
    }

    pub struct Job(HANDLE);

    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn new() -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                Some(Self(handle))
            }
        }

        /// # Safety
        /// `process` must be a valid, open process handle.
        pub unsafe fn assign(&self, process: HANDLE) -> bool {
            unsafe { AssignProcessToJobObject(self.0, process) != 0 }
        }

        pub fn terminate(&self) -> bool {
            unsafe { TerminateJobObject(self.0, 1) != 0 }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn interrupt(_pid: u32, _helper: Option<&Path>) -> ControlResult {
    Err(ControlError::Unsupported)
}

#[cfg(not(any(unix, windows)))]
pub fn kill(_pid: u32) -> ControlResult {
    Err(ControlError::Unsupported)
}
