use std::io;
use libc::{c_long, c_ulonglong, user_regs_struct};
use nix::sys::ptrace::{getevent, getregs};
use nix::sys::signal::kill;
use nix::unistd::Pid;

enum Arch {
    X86,
    X86_64
}

pub(crate) struct Tracee {
    pub(crate) pid: Pid,
    pub(crate) regs: Option<user_regs_struct>,
    pub(crate) arch: Option<Arch>,
}

impl Tracee {
    fn new(pid: Pid) -> Self {
        let mut result = Tracee {
            pid,
            regs: None,
            arch: None
        };

        if result.is_alive() {
            result.regs = Some(getregs(pid).expect("failed to read tracee registers"));
        }

        result
    }

    fn is_alive(&self) -> bool {
        kill(self.pid, None).is_ok()
    }

    fn get_event_msg(&self) -> io::Result<c_long> {
        Ok(getevent(self.pid)?)
    }

    fn get_syscall_number(&self) -> Option<c_ulonglong> {
        self.regs.map(|regs| regs.orig_rax)
    }

    fn get_syscall_argument(&self) {
        
    }
}