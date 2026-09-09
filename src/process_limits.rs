use anyhow::Result;

const DESIRED_OPEN_FILE_LIMIT: u64 = 65_536;

#[cfg(unix)]
pub fn raise_open_file_limit() -> Result<()> {
    let mut limits = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: limits is valid for the complete getrlimit call.
    let get_result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) };
    if get_result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    let desired = limits.rlim_max.min(DESIRED_OPEN_FILE_LIMIT as libc::rlim_t);
    if limits.rlim_cur >= desired {
        return Ok(());
    }

    limits.rlim_cur = desired;
    // SAFETY: limits contains values returned by getrlimit.
    let set_result = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limits) };
    if set_result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(())
}

#[cfg(not(unix))]
pub fn raise_open_file_limit() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn raises_small_open_file_limit() {
        let result = super::raise_open_file_limit();

        assert!(result.is_ok());
    }
}
