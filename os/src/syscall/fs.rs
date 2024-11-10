//! File and filesystem-related syscalls
use crate::fs::{linkat, open_file, unlinkat, OpenFlags, Stat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
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
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
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
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    // 使用 translated_byte_buffer 获取对应内核空间的缓冲区可变引用
    let mut buffers = translated_byte_buffer(token, _st as *const u8, core::mem::size_of::<Stat>());
    if _fd >= inner.fd_table.len() {
        return -1;
    }
    log::debug!("fd <= fd_table.len");
    if let Some(file) = &inner.fd_table[_fd] {
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let (ino, mode, nlink) = file.get_metadata();
        log::debug!("get_metadata success in sys_fstat");
        let temp = Stat::init(ino, mode, nlink);
        // 生成Stat的字节数组
        let temp_slice = unsafe {
            core::slice::from_raw_parts(
                &temp as *const _ as *const u8,
                core::mem::size_of::<Stat>(),
            )
        };

        // let mut written = 0;
        // for buffer in buffers.into_iter() {
        //     log::debug!("buffers length {}", buffers.len());
        //     let remaining = core::mem::size_of::<Stat>() - written;
        //     let write_size = remaining.min(buffer.len());
        //     buffer[..write_size].copy_from_slice(&temp_slice[written..written + write_size]);
        //     written += write_size;
        //     if written == core::mem::size_of::<Stat>() {
        //         return 0; // 成功写入全部数据
        //     }
        // }
        buffers[0].copy_from_slice(temp_slice);
        log::debug!("exit loop in sys_fstat");
        0
        // if written == 0 {
        //     panic!("nothing has written!"); // 没有写入任何数据
        // } else {
        //     panic!("written has not completed"); // 只写入了部分数据
        // }
    } else {
        log::debug!("fd_table(fd) fail");
        -1
    }
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let old_name = translated_str(token, _old_name);
    let new_name = translated_str(token, _new_name);
    linkat(old_name.as_str(), new_name.as_str())
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let name = translated_str(token, _name);
    unlinkat(name.as_str())
}
