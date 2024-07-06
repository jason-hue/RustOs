#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]
#![feature(fn_traits)]

extern crate alloc;

#[macro_use]
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;

#[macro_use]
mod console;
mod config;
mod drivers;
mod fs;
mod fatfs;
mod lang_items;
mod mm;
mod sbi;
mod sync;
mod syscall;
mod task;
mod timer;
mod trap;
mod ext4fs;
mod ext4fs_interface;
use core::arch::global_asm;
use crate::ext4fs_interface::{init_dt, parse_dtb_size};
use crate::mm::{ kernel_token, translated_pa_to_va};
use crate::task::current_user_token;

global_asm!(include_str!("entry.asm"));

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

#[no_mangle]
pub fn rust_main(device_tree_paddr: usize) -> ! {
    let dtb_addr = device_tree_paddr;
    clear_bss();
    let dtb_size = parse_dtb_size(dtb_addr);
    println!("[kernel] Hello, world!");
    mm::init();
    mm::remap_test();
    trap::init();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    println!("TDB_addr: {:#x}",dtb_addr);
    init_dt(dtb_addr);
    fatfs::fs_init();
    task::add_initproc();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}
