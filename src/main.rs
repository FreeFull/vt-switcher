use eyre::{eyre, Context, Result};
use signal_hook::consts::{SIGUSR1, SIGUSR2, TERM_SIGNALS};
use signal_hook::iterator::Signals;
use signal_hook::low_level::signal_name;
use std::{fs::OpenOptions, os::fd::AsRawFd};

fn main() -> Result<()> {
    let handler = Handler::register()?;
    for signal in handler.signals()?.forever() {
        match signal {
            SIGUSR1 => {
                println!("Acquire");
                handler.acquire()?;
            }
            SIGUSR2 => {
                println!("Release");
                handler.release()?;
            }
            signal if TERM_SIGNALS.contains(&signal) => {
                println!(
                    "{} received, terminating",
                    signal_name(signal).unwrap_or("")
                );
                break;
            }
            signal => {
                println!(
                    "Unexpected signal: {signal} {}",
                    signal_name(signal).unwrap_or("")
                );
            }
        }
    }
    handler.restore()?;
    Ok(())
}

struct Handler {
    vt: std::fs::File,
    old_mode: ffi::vt_mode,
}

impl Handler {
    fn register() -> Result<Self> {
        use signal_hook::consts::signal::*;
        use std::ffi::c_short;

        let vt = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .wrap_err("Failed to open /dev/tty")?;

        let mut old_mode = ffi::vt_mode::default();
        unsafe {
            ffi::vt_getmode(vt.as_raw_fd(), &mut old_mode)
                .map_err(|err| eyre!("Could not get VT mode, /dev/tty is not a VT: {err:?}"))?;
            println!("Old mode: {:?}", old_mode);
            let new_mode = ffi::vt_mode {
                mode: ffi::VT_PROCESS,
                waitv: 0,
                relsig: SIGUSR2 as c_short,
                acqsig: SIGUSR1 as c_short,
                frsig: 0,
            };
            println!("New mode: {:?}", new_mode);
            ffi::vt_setmode(vt.as_raw_fd(), &new_mode)
                .map_err(|err| eyre!("Could not set VT mode, {err:?}"))?;
        }

        Ok(Handler { vt, old_mode })
    }

    fn signals(&self) -> Result<Signals> {
        Ok(Signals::new([SIGUSR1, SIGUSR2].iter().chain(TERM_SIGNALS))?)
    }

    fn acquire(&self) -> Result<()> {
        use std::ffi::c_int;
        unsafe {
            ffi::vt_reldisp(self.vt.as_raw_fd(), ffi::VT_ACKACQ as c_int)?;
        }
        Ok(())
    }

    fn release(&self) -> Result<()> {
        unsafe {
            ffi::vt_reldisp(self.vt.as_raw_fd(), 1)?;
        }
        Ok(())
    }

    fn restore(self) -> Result<()> {
        unsafe {
            ffi::vt_setmode(self.vt.as_raw_fd(), &self.old_mode)
                .map_err(|err| eyre!("Failed to restore VT mode: {err:?}"))?;
        }
        Ok(())
    }
}

mod ffi {
    use std::ffi::{c_char, c_short};

    #[allow(non_camel_case_types)]
    #[repr(C)]
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct vt_mode {
        pub mode: c_char,
        pub waitv: c_char,
        pub relsig: c_short,
        pub acqsig: c_short,
        pub frsig: c_short, // Unused, always set to 0
    }

    pub const VT_AUTO: c_char = 0;
    pub const VT_PROCESS: c_char = 1;

    pub const VT_ACKACQ: c_char = 2;

    nix::ioctl_read_bad!(vt_getmode, 0x5601, vt_mode);
    nix::ioctl_write_ptr_bad!(vt_setmode, 0x5602, vt_mode);
    nix::ioctl_write_int_bad!(vt_reldisp, 0x5605);
}
