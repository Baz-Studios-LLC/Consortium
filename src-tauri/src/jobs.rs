// Making sure agent processes cannot outlive Consortium.
//
// Closing the window stops the agents deliberately, and that covers the normal
// case. It does not cover the abnormal one: a crash, a kill from Task Manager,
// a machine losing power. On Windows a child process does not die with its
// parent, so any of those leaves an agent running with nothing attached to it —
// invisible, holding a model session open, and joined by another one the next
// time Consortium starts.
//
// A job object fixes that at the level where it can actually be guaranteed. The
// operating system, not Consortium, kills the children, and it does so however
// Consortium ends — including the ways that never get to run any code.
//
// Deliberately not a heartbeat. The child here is Codex's own app-server, which
// we do not control and cannot ask to check whether we are still alive; and a
// process that polls to find out whether to exist is the shape of thing this
// project has spent the day removing.
//
// Everything here is a no-op off Windows. macOS needs its own answer — a
// process group, most likely — and until it has one, the Mac build relies on
// the clean-close path alone. Said plainly rather than left to be discovered.

/// Puts a child process under Consortium's lifetime.
///
/// Failure is reported and not fatal: an agent that could not be adopted still
/// works, it just needs the clean-close path to shut it down. Refusing to start
/// an agent over this would trade a rare leak for a certain outage.
pub fn adopt(child: &std::process::Child) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows_impl::adopt(child)
    }
    #[cfg(not(windows))]
    {
        let _ = child;
        Ok(())
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JobObjectExtendedLimitInformation,
    };

    /// The job every agent process is put into.
    ///
    /// Held for the life of the process and never closed by us: the kill is
    /// triggered by the last handle to the job going away, which is exactly what
    /// happens when Consortium exits by any route at all. Stored as isize
    /// because a raw HANDLE is a pointer and not Send.
    static JOB: OnceLock<isize> = OnceLock::new();

    fn job() -> Option<HANDLE> {
        let handle = JOB.get_or_init(|| {
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return 0;
            }

            // The whole point: when the last handle to this job closes, every
            // process still in it is terminated.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ok = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            // A job without the limit set would adopt children and never kill
            // them, which is worse than not having one: it would look handled.
            if ok == 0 {
                return 0;
            }
            job as isize
        });

        (*handle != 0).then_some(*handle as HANDLE)
    }

    pub fn adopt(child: &std::process::Child) -> Result<(), String> {
        let Some(job) = job() else {
            return Err("could not create a job object".into());
        };

        let ok = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) };
        if ok == 0 {
            return Err(format!(
                "could not adopt process {}: {}",
                child.id(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}
