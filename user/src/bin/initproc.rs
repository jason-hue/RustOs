#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use user_lib::{exec, fork, wait, yield_};

#[no_mangle]
fn main() -> i32 {
    let file: [&str; 26] =["exit\0","clone\0","close\0","dup\0","fork\0","fstat\0","chdir\0","getcwd\0","getpid\0","getppid\0","gettimeofday\0","mkdir_\0",
                       "mmap\0","munmap\0","read\0","sleep\0","times\0","wait\0","waitpid\0","execve\0","write\0","open\0","uname\0","yield\0","openat\0","pipe\0"];
    //let file:[&str;2]=["openat\0","exit\0"];
    for f in &file{
        fe(f);
    }

    0
}

fn fe(file: &str){
    if fork()==0{
        exec(file,&[core::ptr::null::<u8>()]);
    }else {
        let mut exit_code: i32 = 0;
        let pid = wait(&mut exit_code);
        return;
    }
}