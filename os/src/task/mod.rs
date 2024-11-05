//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::config::MAX_SYSCALL_NUM;
use crate::loader::{get_app_data, get_num_app};
use crate::mm::{self, MapPermission, VirtPageNum};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// 增加当前任务对应系统调用次数
    fn increase_current_syscall(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current_id = inner.current_task;
        let current_task = &mut inner.tasks[current_id];
        current_task.syscall_times[syscall_id] += 1;
    }

    ///返回当前任务的系统调用次数统计数组
    fn get_task_syscall_times(&self) -> [u32; MAX_SYSCALL_NUM] {
        let inner = self.inner.exclusive_access();
        let current_id = inner.current_task;
        inner.tasks[current_id].syscall_times
    }

    ///返回当前任务的开始时间
    fn get_task_start_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current_id = inner.current_task;
        inner.tasks[current_id].task_start_time
    }

    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        // 记录第一个任务启动的时间
        next_task.task_start_time = get_time_ms();
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            // 记录任务第一次被调度的时间
            if inner.tasks[current].task_start_time == 0 {
                inner.tasks[current].task_start_time = get_time_ms();
            }
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }
    fn mmap(&self, start: usize, len: usize, port: usize) -> isize {
        // 获取地址空间
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let m_set = &mut inner.tasks[current].memory_set;

        // 检查起始地址是否页对齐和port的合法性（除低3位外其余全为0且低3位不能全为0）
        if (start & 0xFFF) != 0 || port & !0x7 != 0 || port & 0x7 == 0 {
            println!("invaild address or port");
            return -1;
        }

        // 检查地址范围内是否存在已经映射的页
        let start_va = mm::VirtAddr::from(start);
        let end_va = mm::VirtAddr::from(start + len);
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();

        for vpn in mm::VPNRange::new(start_vpn, end_vpn) {
            if let Some(pte) = m_set.translate(vpn) {
                // 已经被映射过了
                if pte.is_valid() {
                    println!("address already mapped");
                    return -1;
                };
            }
        }

        // 将port转换成MapPermission
        // MapPermission是从第1位开始的，所以port要左移1位，还要注意U位置1
        let flags = MapPermission::from_bits((port << 1) as u8).unwrap() | MapPermission::U;

        // 以逻辑段为单位将该地址范围加入到应用的地址空间中
        // 函数内部也是按页处理的
        m_set.insert_framed_area(start_va, end_va, flags);

        0
    }

    /// 取消内存映射
    fn munmap(&self, start: usize, len: usize) -> isize {
        // 检查start是否对齐
        if start & 0xFFF != 0 {
            println!("invaild address or port");
            return -1;
        }

        // 获取地址空间
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let m_set = &mut inner.tasks[current].memory_set;

        // 检查地址范围内是否存在没被映射的页
        let start_va = mm::VirtAddr::from(start);
        let end_va = mm::VirtAddr::from(start + len);
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();

        for vpn in mm::VPNRange::new(start_vpn, end_vpn) {
            if let Some(pte) = m_set.translate(vpn) {
                if !pte.is_valid() {
                    println!("exists address not mapped");
                    return -1;
                };
            }
        }
        m_set.del_framed_area(start_va, end_va);
        0
    }
}
/// 申请内存
pub fn mmap(start: usize, len: usize, port: usize) -> isize {
    TASK_MANAGER.mmap(start, len, port)
}
/// 取消内存映射
pub fn munmap(start: usize, len: usize) -> isize {
    TASK_MANAGER.munmap(start, len)
}

/// 增加当前任务对应系统调用的次数
pub fn increase_current_syscall(syscall_id: usize) {
    TASK_MANAGER.increase_current_syscall(syscall_id);
}
///返回当前任务的系统调用次数统计数组
pub fn get_task_syscall_times() -> [u32; MAX_SYSCALL_NUM] {
    TASK_MANAGER.get_task_syscall_times()
}

///返回当前任务的开始时间
pub fn get_task_start_time() -> usize {
    TASK_MANAGER.get_task_start_time()
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}
