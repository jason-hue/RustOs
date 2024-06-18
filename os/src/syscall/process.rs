use crate::fs::{open_file, OpenFlags};
use crate::mm::{translated_ref, translated_refmut, translated_str, VirtAddr};
use crate::task::{
    current_process, current_task, current_user_token, exit_current_and_run_next, pid2process,
    suspend_current_and_run_next, SignalFlags,
};
use crate::timer::{get_time_ms, get_time_us, USEC_PER_SEC};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().process.upgrade().unwrap().getpid() as isize
}

pub fn sys_fork() -> isize {
    let current_process = current_process();
    let new_process = current_process.fork();
    let new_pid = new_process.getpid();
    // modify trap context of new_task, because it returns immediately after switching
    let new_process_inner = new_process.inner_exclusive_access();
    let task = new_process_inner.tasks[0].as_ref().unwrap();
    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    new_pid as isize
}

pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let process = current_process();
        let argc = args_vec.len();
        process.exec(all_data.as_slice(), args_vec);
        // return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let process = current_process();
    // find a child process

    let mut inner = process.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

pub fn sys_kill(pid: usize, signal: u32) -> isize {
    if let Some(process) = pid2process(pid) {
        if let Some(flag) = SignalFlags::from_bits(signal) {
            process.inner_exclusive_access().signals |= flag;
            0
        } else {
            -1
        }
    } else {
        -1
    }
}
pub fn sys_get_time_of_day(ts: *mut usize) -> isize {
    let token = current_user_token();
    let us = get_time_us();
    let sec = us / USEC_PER_SEC;
    let usec = us % USEC_PER_SEC;
    *translated_refmut(token, ts) = sec;
    *translated_refmut(token, unsafe { ts.add(1) }) = usec;
    0
}
pub fn sys_getppid() -> isize {
    let ppid = current_process()
        .inner_exclusive_access()
        .parent
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .getpid() as isize;
    // println!("ppid: {:}", ppid);
    // println!("pid: {:}", sys_getpid());
    ppid
}
pub fn sys_clone(_flags: usize, stack_ptr: usize, _ptid: usize, _tls: usize, _ctid: usize) -> isize {
    let current_process = current_process();
    let child_process = current_process.fork();
    let child_pic = child_process.getpid();
    // modify trap context of new_task, because it returns immediately after switching
    let child_process_inner = child_process.inner_exclusive_access();
    let task = child_process_inner.tasks[0].as_ref().unwrap();
    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    if stack_ptr != 0 {
        trap_cx.x[2] = stack_ptr;
    }
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    child_pic as isize
}
#[allow(unused)]
pub fn sys_wait4(pid: isize, exit_code_ptr: *mut i32, _options: isize) -> isize {
    println!("enter wait");
    loop {
        let process = current_process();
        // find a child process

        let mut inner = process.inner_exclusive_access();
        if !inner
            .children
            .iter()
            .any(|p| pid == -1 || pid as usize == p.getpid())
        {
            return -1;
            // ---- release current PCB
        }
        let pair = inner.children.iter().enumerate().find(|(_, p)| {
            // ++++ temporarily access child PCB exclusively
            p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
            // ++++ release child PCB
        });
        if let Some((idx, _)) = pair {
            let child = inner.children.remove(idx);
            // confirm that child will be deallocated after being removed from children list
            assert_eq!(Arc::strong_count(&child), 1);
            let found_pid = child.getpid();
            // ++++ temporarily access child PCB exclusively
            let exit_code = child.inner_exclusive_access().exit_code;
            // ++++ release child PCB
            if exit_code_ptr as usize != 0 {
                // 返回到用户的时候会调用函数右移8位
                *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code << 8;
            }
            return found_pid as isize;
        } else {
            drop(inner);
            suspend_current_and_run_next();
        }
    }
    // ---- release current PCB automatically
}
/// increase brk_pt(uheap)
/// if new_brk == 0 return task.res.brk_pt(different from other new_brk val)
/// if new_brk != 0 update task.res.brk to new_brk
/// if not success return -1, if success return 0
pub fn sys_brk(new_brk: usize) -> isize {
    // current_process().inner_exclusive_access().memory_set.grow_proc(VirtAddr::from(new_brk))
    0
}
/// map file to memory
/// if start is 0 -> the memory's start is mmap_top
/// if start is not 0 -> the memory's start must align
// pub fn sys_mmap(
//     start: usize,
//     len: usize,
//     prot: usize,
//     flags: usize,
//     fd: isize,
//     offset: usize,
// ) -> isize {
//     // println!("start: 0x{:x}, len: 0x{:}, fd: {:}", start, len, fd);
//     let process = current_process();
//     let mut inner = process.inner_exclusive_access();
//     let align_start = if start == 0 {
//         inner.memory_set.mmap_top.0
//     } else {
//         align_down(start)
//     };
//     let align_len = align_up(len);
//     if align_start < USER_MMAP_BOTTOM || align_start + align_len >= USER_MMAP_BOTTOM + RLIMIT_MMAP {
//         return MMAP_FAILED;
//     }
//     // println!("align_start: 0x{:x}, align_len: 0x{:x}", align_start, align_len);
//     inner.memory_set.mmap(align_start, align_len, prot, flags, fd, offset);
//     align_start as isize
// }
// pub struct tms
// {
// 	tms_utime:  usize,
// 	tms_stime:  usize,
// 	tms_cutime: usize,
// 	tms_cstime: usize,
// }
// tms 指向的就是tms结构
pub fn sys_times(tms: *mut usize) -> isize {
    let token = current_user_token();
    let usec = get_time_us();
    *translated_refmut(token, tms) = usec;
    *translated_refmut(token, unsafe { tms.add(1) }) = usec;
    *translated_refmut(token, unsafe { tms.add(2) }) = usec;
    *translated_refmut(token, unsafe { tms.add(3) }) = usec;
    usec as isize
}