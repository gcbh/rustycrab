//! Cross-platform sandboxed subprocess execution.
//!
//! Provides OS-level process isolation for tools that spawn subprocesses
//! (primarily `CodeExecutionTool`). Two layers of protection:
//!
//! 1. **POSIX resource limits** (`setrlimit`): memory, CPU, file size, and
//!    process count limits. Works on macOS and Linux.
//!
//! 2. **Platform-specific isolation**:
//!    - **macOS**: Seatbelt (`sandbox-exec`) profiles that restrict filesystem
//!      writes and deny network access at the kernel level.
//!    - **Linux**: Namespace isolation (`unshare`) for PID, IPC, and network,
//!      plus `PR_SET_NO_NEW_PRIVS` to prevent privilege escalation.

use std::io;
use std::path::Path;

/// Apply POSIX resource limits to the current process.
///
/// Intended to be called from a `pre_exec` hook so that limits are applied
/// to the child process before it execs the target binary.
///
/// # Safety
///
/// Must be called in a `pre_exec` context (post-fork, pre-exec). Uses only
/// async-signal-safe libc calls (`setrlimit`).
#[cfg(unix)]
pub fn apply_resource_limits(max_memory_bytes: u64, max_cpu_secs: u64) -> io::Result<()> {
    unsafe {
        // Memory limit (virtual address space)
        if max_memory_bytes > 0 {
            let limit = libc::rlimit {
                rlim_cur: max_memory_bytes,
                rlim_max: max_memory_bytes,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        // CPU time limit
        if max_cpu_secs > 0 {
            let limit = libc::rlimit {
                rlim_cur: max_cpu_secs,
                rlim_max: max_cpu_secs,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, &limit) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        // File size limit (10 MB)
        let fsize_limit = libc::rlimit {
            rlim_cur: 10 * 1024 * 1024,
            rlim_max: 10 * 1024 * 1024,
        };
        if libc::setrlimit(libc::RLIMIT_FSIZE, &fsize_limit) != 0 {
            return Err(io::Error::last_os_error());
        }

        // Process count limit — prevent fork bombs.
        // Set to 1 so the Python process itself can run but cannot spawn children.
        let nproc_limit = libc::rlimit {
            rlim_cur: 1,
            rlim_max: 1,
        };
        if libc::setrlimit(libc::RLIMIT_NPROC, &nproc_limit) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Generate a macOS Seatbelt sandbox profile for code execution.
///
/// The profile denies all operations by default, then selectively allows:
/// - Reading system libraries, Python installation, and the sandbox directory
/// - Writing only to the sandbox temporary directory
/// - Basic process operations needed by the Python runtime
/// - Network access is always denied
#[cfg(target_os = "macos")]
pub fn generate_seatbelt_profile(sandbox_dir: &Path, python_path: &Path) -> String {
    let sandbox_dir = sandbox_dir.to_string_lossy();

    // Resolve the Python installation's lib directory for read access.
    // e.g., /opt/homebrew/Cellar/python@3.12/3.12.x/Frameworks/...
    let python_parent = python_path
        .parent()
        .and_then(|p| p.parent()) // go up from bin/ to the install root
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut profile = String::from(
        r#"(version 1)
(deny default)

;; Allow the Python process to execute
(allow process-exec)

;; Allow reading system libraries and frameworks
(allow file-read*
    (subpath "/usr/")
    (subpath "/Library/")
    (subpath "/System/")
    (subpath "/private/var/")
    (subpath "/opt/homebrew/")
    (subpath "/usr/local/")
    (subpath "/dev/")
    (subpath "/private/tmp/")
    (subpath "/tmp/")
"#,
    );

    // Allow reading the Python installation directory (for pyenv, conda, etc.)
    if !python_parent.is_empty()
        && !python_parent.starts_with("/usr/")
        && !python_parent.starts_with("/opt/homebrew/")
        && !python_parent.starts_with("/usr/local/")
    {
        profile.push_str(&format!("    (subpath \"{}\")\n", python_parent));
    }

    profile.push_str(&format!(
        r#"    (subpath "{sandbox_dir}"))

;; Allow writing ONLY to the sandbox temp directory
(allow file-write*
    (subpath "{sandbox_dir}")
    (subpath "/dev/null"))

;; Allow basic process operations needed by Python runtime
(allow process-fork)
(allow signal (target self))
(allow sysctl-read)

;; Allow mach IPC (required for basic process operation on macOS)
(allow mach-lookup)
(allow mach-register)

;; DENY all network access
(deny network*)
"#
    ));

    profile
}

/// Apply Linux namespace isolation to the current process.
///
/// Called from `pre_exec` to isolate the child process. Falls back
/// gracefully if namespaces are unavailable (rlimits still apply).
///
/// # Safety
///
/// Must be called in a `pre_exec` context. Uses only libc syscalls.
#[cfg(target_os = "linux")]
pub fn apply_linux_namespaces() -> io::Result<()> {
    unsafe {
        // Prevent privilege escalation via setuid binaries
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            // Non-fatal: log and continue
            let _ = io::Error::last_os_error();
        }

        // Create new namespaces: PID, IPC, and network
        let flags = libc::CLONE_NEWPID | libc::CLONE_NEWIPC | libc::CLONE_NEWNET;
        if libc::unshare(flags) != 0 {
            // Namespace isolation unavailable (e.g., user namespaces disabled).
            // Fall back to rlimits-only protection. This is logged as a warning
            // by the caller.
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    use std::path::Path;

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_profile_denies_network() {
        let profile = generate_seatbelt_profile(
            Path::new("/tmp/rustykrab_sandbox"),
            Path::new("/usr/bin/python3"),
        );
        assert!(profile.contains("(deny network*)"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_profile_allows_sandbox_dir_write() {
        let profile = generate_seatbelt_profile(
            Path::new("/tmp/rustykrab_sandbox"),
            Path::new("/usr/bin/python3"),
        );
        assert!(profile.contains("(subpath \"/tmp/rustykrab_sandbox\")"));
        assert!(profile.contains("(allow file-write*"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_profile_includes_custom_python_path() {
        let profile = generate_seatbelt_profile(
            Path::new("/tmp/rustykrab_sandbox"),
            Path::new("/Users/dev/.pyenv/versions/3.12.0/bin/python3"),
        );
        // Should include the pyenv install root for read access
        assert!(profile.contains("/Users/dev/.pyenv/versions/3.12.0"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_profile_no_duplicate_for_system_python() {
        let profile = generate_seatbelt_profile(
            Path::new("/tmp/rustykrab_sandbox"),
            Path::new("/usr/bin/python3"),
        );
        // /usr/ is already in the default paths, so no extra subpath needed
        let count = profile.matches("(subpath \"/usr/\")").count();
        assert_eq!(count, 1);
    }
}
