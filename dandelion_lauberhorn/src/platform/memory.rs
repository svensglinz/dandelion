use std::fs::read_to_string;

const BYTES_PER_KB: usize = 1024;

/// Return the total system RAM in bytes (read from `/proc/meminfo`).
pub fn get_max_ram() -> Result<usize, ()> {
    let meminfo = read_to_string("/proc/meminfo").map_err(|_| ())?;
    let kb = meminfo
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|rest| rest.strip_suffix("kB"))
                .and_then(|rest| rest.trim().parse::<usize>().ok())
        })
        .ok_or(())?;
    Ok(kb * BYTES_PER_KB)
}