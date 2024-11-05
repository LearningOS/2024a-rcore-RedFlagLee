//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    mm::translated_byte_buffer,
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next, get_task_start_time,
        get_task_syscall_times, mmap, munmap, suspend_current_and_run_next, TaskStatus,
    },
    timer::{get_time_ms, get_time_us},
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    // 获取当前任务的 token
    let token = current_user_token();

    // 使用 translated_byte_buffer 获取用户空间的缓冲区可变引用
    let buffers = translated_byte_buffer(token, ts as *const u8, core::mem::size_of::<TimeVal>());

    let us = get_time_us();
    let time = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    // 生成TimeVal的字节数组
    let time_slice = unsafe {
        core::slice::from_raw_parts(
            &time as *const _ as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };

    let mut written = 0;
    for buffer in buffers.into_iter() {
        let remaining = core::mem::size_of::<TimeVal>() - written;
        let write_size = remaining.min(buffer.len());
        buffer[..write_size].copy_from_slice(&time_slice[written..written + write_size]);
        written += write_size;
        if written == core::mem::size_of::<TimeVal>() {
            return 0; // 成功写入全部数据
        }
    }

    if written == 0 {
        panic!("nothing has written!"); // 没有写入任何数据
    } else {
        panic!("written has not completed"); // 只写入了部分数据
    }
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    // 获取当前任务的 token
    let token = current_user_token();

    // 使用 translated_byte_buffer 获取用户空间的缓冲区可变引用
    let buffers = translated_byte_buffer(token, ti as *const u8, core::mem::size_of::<TaskInfo>());

    let task_info = TaskInfo {
        status: TaskStatus::Running,
        syscall_times: get_task_syscall_times(),
        time: get_time_ms() - get_task_start_time(),
    };
    println!(
        "info.syscall_times[SYSCALL_GETTIMEOFDAY] = {}",
        task_info.syscall_times[169]
    );
    println!(
        "info.syscall_times[SYSCALL_YIELD] = {}",
        task_info.syscall_times[124]
    );
    println!(
        "info.syscall_times[SYSCALL_TASK_INFO] = {}",
        task_info.syscall_times[410]
    );

    let task_slice = unsafe {
        core::slice::from_raw_parts(
            &task_info as *const TaskInfo as *const u8,
            core::mem::size_of::<TaskInfo>(),
        )
    };

    let mut written = 0;
    for buffer in buffers.into_iter() {
        let remaining = core::mem::size_of::<TaskInfo>() - written;
        let write_size = remaining.min(buffer.len());
        buffer[..write_size].copy_from_slice(&task_slice[written..written + write_size]);
        written += write_size;
        if written == core::mem::size_of::<TaskInfo>() {
            return 0; // 成功写入全部数据
        }
    }

    if written == 0 {
        panic!("nothing has written!"); // 没有写入任何数据
    } else {
        panic!("written has not completed"); // 只写入了部分数据
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    mmap(_start, _len, _port)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    munmap(_start, _len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
