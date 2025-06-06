use std::{fs, path::PathBuf};

use anyhow::Result;
use time::{OffsetDateTime, macros::format_description};

fn main() -> Result<()> {
    // read env variables that were set in build script
    let uefi_path = env!("UEFI_PATH");
    let bios_path = env!("BIOS_PATH");

    // choose whether to start the UEFI or BIOS image
    let uefi = true;

    let log_file = PathBuf::from(OffsetDateTime::now_local()?.format(format_description!(
        "logs/[year]-[month]-[day]/\
        [hour]-[minute]-[second]Z[offset_hour sign:mandatory][offset_minute].log"
    ))?);

    match fs::create_dir_all(log_file.parent().unwrap()) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err.into()),
    }

    match fs::remove_file("logs/last.log") {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    std::os::unix::fs::symlink(log_file.strip_prefix("logs/")?, "logs/last.log")?;

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd.args(["-enable-kvm", "-s", "-m", "8G"]);
    cmd.args(["-serial", &format!("file:{}", log_file.display())]);

    if uefi {
        cmd.args([
            "-drive",
            &format!("format=raw,file={uefi_path}"),
            "-drive",
            "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd",
            "-drive",
            "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_VARS.fd",
        ]);
    } else {
        cmd.args(["-drive", &format!("format=raw,file={bios_path}")]);
    }
    let mut child = cmd.spawn()?;
    child.wait()?;

    Ok(())
}
