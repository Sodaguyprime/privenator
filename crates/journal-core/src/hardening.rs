//! Keep secrets out of crash dumps (docs/SPEC.md "Rules"). Call once at startup,
//! before any key is in memory. Best effort: failures are reported, not fatal.

/// Returns a description of each step that failed, for logging.
pub fn harden_process() -> Vec<String> {
    let mut failures = Vec::new();
    imp::run(&mut failures);
    failures
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SetErrorMode};
    use windows_sys::Win32::System::ErrorReporting::WerAddExcludedApplication;

    pub fn run(failures: &mut Vec<String>) {
        // No crash dialog (which would also trigger a WER report).
        unsafe { SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX) };

        // Exclude this executable from Windows Error Reporting for the current user.
        match std::env::current_exe() {
            Ok(exe) => {
                let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
                let hr = unsafe { WerAddExcludedApplication(wide.as_ptr(), 0) };
                if hr < 0 {
                    failures.push(format!("WerAddExcludedApplication failed: 0x{hr:08x}"));
                }
            }
            Err(e) => failures.push(format!("current_exe: {e}")),
        }
    }
}

#[cfg(unix)]
mod imp {
    pub fn run(failures: &mut Vec<String>) {
        // No core files on any Unix.
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &zero) } != 0 {
            failures.push("setrlimit(RLIMIT_CORE, 0) failed".into());
        }
        // Linux/Android: also blocks ptrace attach by other same-user processes.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
            failures.push("prctl(PR_SET_DUMPABLE, 0) failed".into());
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    pub fn run(failures: &mut Vec<String>) {
        failures.push("no process hardening for this platform".into());
    }
}
