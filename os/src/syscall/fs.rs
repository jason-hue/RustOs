use alloc::string::ToString;
use crate::fs::{File, FileDescriptor, Kstat, make_pipe, open_file, OpenFlags};
use crate::mm::{translated_byte_buffer, translated_bytes, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process, current_user_token};
use alloc::sync::Arc;
use core::mem::size_of;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(fd:isize, path: *const u8, flags: u32) -> isize {
    print!("");
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let path = translated_str(token, path).replace("./", "");
    let flag = OpenFlags::from_bits(flags).unwrap();
    let (readable, writable) = flag.read_write();
    let dir = if fd >= 0 {
        let fd = fd as usize;
        if let Some(dir) = inner.fd_table.get(fd).unwrap() {
            dir
        } else {
            return -1;
        }
    } else {
        &inner.work_dir.as_ref()
    };
    //打开当前
    if path == ".".to_string() {
        let tmp = dir.clone();
        drop(dir);
        let fd = inner.alloc_fd();
        inner.fd_table.insert(fd, Some(tmp));
        return fd as isize;
    }
    let flag = OpenFlags::from_bits(flags).unwrap();
    let file = if flag.contains(OpenFlags::CREATE) {
        dir.create(&path, readable, writable, false)
    } else {
        open_file(&path, flag)
        //dir.open(&path, readable, writable, directory)
    };
    if let Some(file) = file {
        let fd = inner.alloc_fd();
        inner.fd_table.insert(fd, Some(FileDescriptor::File(file)));
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    println!("Enter close");
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

pub fn sys_pipe(pipe: *mut usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(FileDescriptor::Abstract(pipe_read));
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(FileDescriptor::Abstract(pipe_write));
    *translated_refmut(token, pipe) = read_fd;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    0
}

pub fn sys_dup(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(inner.fd_table[fd].as_ref().unwrap().clone());
    new_fd as isize
}
pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let path = translated_str(token, path).replace("./", "");
    let inode = inner
        .work_dir
        .as_ref()
        .open(&path, true, true, true);
    match inode {
        Some(file) => {
            inner.work_dir = Arc::new(FileDescriptor::File (file));
            0
        }
        None => 1,
    }
}
pub fn sys_getcwd(buf: *const u8, size: usize) -> isize{
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let name = inner.work_dir.name().clone();
    let dir = name.as_bytes();
    for b in UserBuffer::new(translated_byte_buffer(token, buf, size)).buffers {
        b[0..dir.len()].copy_from_slice(dir);
    }
    1
}
pub fn sys_dup3(old_fd:isize, new_fd:isize) -> isize{
    -1
}
//未实现组机制，没有mode
pub fn sys_mkdirat(dirfd:isize, path:*const u8, _mode:isize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let path_ = translated_str(token, path).replace("./", "");   //是相对路经时才会更改
    let dir = if dirfd >= 0 {
        if let Some(dir) = inner.fd_table.get(dirfd as usize).unwrap() {
            dir
        } else {
            return -1;
        }
    } else {
        &inner.work_dir
    };
    if let Some(file) = dir.create(&path_, false, false, true) {
        let fd = inner.alloc_fd();
        inner.fd_table.insert(fd, Some(FileDescriptor::File(file)));
        fd as isize
    } else {
        -1
    }

}

pub fn sys_fstat(fd: isize, ptr: *const u8) -> isize {
    let token = current_user_token();
    let mut user_buf = UserBuffer::new(translated_byte_buffer(token, ptr, core::mem::size_of::<Kstat>()));
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if let Some(opt) = inner.fd_table.get(fd as usize) {
        if let Some(file) = opt {
            let mut stat = Kstat::default();
            file.kstat(&mut stat);
            user_buf.write(translated_bytes(&stat,size_of::<Kstat>()));
            // println!("{:?}",stat);
            return 0;
        }
    }
    1
}