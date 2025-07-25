use libc::{getpid, gettid, pid_t, syscall, SYS_tgkill, SIGUSR1};
use linear_map::set::LinearSet;
use std::ffi::c_int;
use std::mem::zeroed;
use std::sync::{LazyLock, Mutex, Once};
use std::{mem, thread};
use std::time::Duration;
use cvt::cvt;

static mut PREVIOUS_SIGHANDLER: *mut libc::sigaction = 0 as *mut _;
static TIMEOUT_THREAD_DATA: LazyLock<Mutex<LinearSet<pid_t>>> = LazyLock::new(|| Mutex::new(LinearSet::new()));
static TIMEOUT_THREAD: Once = Once::new();

fn init_timeout_thread() {
    thread::spawn(|| {
        unsafe {
            let tgid = getpid();

            loop {
                thread::sleep(Duration::from_millis(1));

                let lock = TIMEOUT_THREAD_DATA.lock().unwrap();
                for tid in lock.iter() {
                    syscall(SYS_tgkill, tgid, *tid, SIGUSR1);
                }
            }
        }
    });

    unsafe {
        let mut sigusr1: libc::sigaction = zeroed();
        sigusr1.sa_sigaction = sigusr1_handler as usize;
        sigusr1.sa_flags = libc::SA_SIGINFO;
        PREVIOUS_SIGHANDLER = Box::into_raw(Box::new(zeroed()));
        cvt(libc::sigaction(SIGUSR1, &sigusr1, PREVIOUS_SIGHANDLER)).expect("failed to set SIGUSR1 handler");
    }
}

pub(crate) fn add_timeout_thread() {
    TIMEOUT_THREAD.call_once(|| init_timeout_thread());
    TIMEOUT_THREAD_DATA.lock().expect("failed to lock TIMEOUT_THREAD_DATA").insert(unsafe { gettid() });
}

pub(crate) fn remove_timeout_thread() {
    // Fails if the tid was not present in the set
    let tid = unsafe { gettid() };
    assert!(TIMEOUT_THREAD_DATA.lock().expect("failed to lock TIMEOUT_THREAD_DATA").remove(&tid));
}

extern "C" fn sigusr1_handler(signum: c_int, info: *mut libc::siginfo_t, ptr: *mut libc::c_void) {
    type FnSigaction = extern "C" fn(c_int, *mut libc::siginfo_t, *mut libc::c_void);
    type FnHandler = extern "C" fn(c_int);

    unsafe {
        let fnptr = (*PREVIOUS_SIGHANDLER).sa_sigaction;
        if fnptr == 0 {
            return;
        }

        if (*PREVIOUS_SIGHANDLER).sa_flags & libc::SA_SIGINFO == 0 {
            let action = mem::transmute::<usize, FnHandler>(fnptr);
            action(signum)
        } else {
            let action = mem::transmute::<usize, FnSigaction>(fnptr);
            action(signum, info, ptr)
        }
    }
}