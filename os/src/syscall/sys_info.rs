use core::mem::size_of;

use crate::{task::current_user_token, };
use crate::mm::{UserBuffer, translated_byte_buffer};

#[allow(unused)]
#[allow(non_camel_case_types)]
pub struct utsname {
    sysname:    [u8; 65],
    nodename:   [u8; 65],
    release:    [u8; 65],
    version:    [u8; 65],
    machine:    [u8; 65],
    domainname: [u8; 65],
}

impl utsname {
    pub fn new() -> Self {
        Self {
            sysname: utsname::str2u8("Runable"),
            nodename: utsname::str2u8("root"),
            release: utsname::str2u8("0.0"),
            version: utsname::str2u8("0.0"),
            machine: utsname::str2u8("RISC-V64"),
            domainname: utsname::str2u8("https://gitlab.eduxiji.net/runable/oskernel2023-Runable"),
        }
    }

    fn str2u8(s: &str) -> [u8; 65] {
        let mut ans = [0u8; 65];
        let bytes = s.as_bytes();
        let len = s.len();
        ans[..len].copy_from_slice(bytes);
        ans
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const _ as usize as *const u8,
                core::mem::size_of::<Self>(),
            )
        }
    }
}


pub fn sys_uname(buf: *mut u8) -> isize {
    let token = current_user_token();
    let mut user_buf = UserBuffer::new(translated_byte_buffer(token, buf, size_of::<utsname>()));
    if user_buf.write(utsname::new().as_bytes()) == 0 {
        -1
    } else {
        0
    }
}