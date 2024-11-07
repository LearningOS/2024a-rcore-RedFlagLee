//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::MAX_SYSCALL_NUM,
    loader::get_app_data_by_name,
    mm::{translated_refmut, translated_str,
        translated_byte_buffer
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, mmap, munmap
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
    syscall_times: [usize; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
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
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
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

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    trace!("kernel: sys_get_time");
    // 获取当前任务的 token
    let token = current_user_token();

    // 使用 translated_byte_buffer 获取用户空间的缓冲区可变引用
    let buffers = translated_byte_buffer(token, _ts as *const u8, core::mem::size_of::<TimeVal>());

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
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // 获取当前任务
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();

    // 获取当前任务的 token
    let token = current_user_token();

    // 使用 translated_byte_buffer 获取用户空间的缓冲区可变引用
    let buffers = translated_byte_buffer(token, _ti as *const u8, core::mem::size_of::<TaskInfo>());

    let task_info = TaskInfo {
        status: TaskStatus::Running,
        syscall_times: inner.syscall_times,
        time: get_time_ms() - inner.task_start_time,
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

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    mmap(_start, _len, _port)
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    munmap(_start, _len)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token=current_user_token();
    let path=translated_str(token, _path);
    if let Some(data)=get_app_data_by_name(path.as_str()){
        let current_task=current_task().unwrap();
        let task=current_task.spawn(data);
        let pid=task.getpid();
        add_task(task);
        pid as isize
    }
    else {
        -1
    }
}


// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio<=1{
        return -1;
    }
    else{
        current_task().unwrap().set_priority(_prio)
    }
}
