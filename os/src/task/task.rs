//! Types related to task management & Functions for completely changing TCB

use super::id::TaskUserRes;
use super::{kstack_alloc, KernelStack, ProcessControlBlock, TaskContext};
use crate::trap::TrapContext;
use crate::{mm::PhysPageNum, sync::UPSafeCell};
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::{Arc, Weak};
use core::cell::RefMut;
use core::num::NonZeroUsize;

const BIG_STRIDE: usize = 10000;

/// Task control block structure
pub struct TaskControlBlock {
    /// immutable
    pub process: Weak<ProcessControlBlock>,
    /// Kernel stack corresponding to PID
    pub kstack: KernelStack,
    /// mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference of the inner TCB
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// Get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }
}

pub struct TaskControlBlockInner {
    pub res: Option<TaskUserRes>,
    /// The physical page number of the frame where the trap context is placed
    pub trap_cx_ppn: PhysPageNum,
    /// Save task context
    pub task_cx: TaskContext,

    /// Maintain the execution status of the current process
    pub task_status: TaskStatus,
    /// It is set when active exit or execution error occurs
    pub exit_code: Option<i32>,

    /// Task information
    pub task_info: TaskInfo,

    /// Stride scheduling
    pub stride: usize,
    pub pass: usize,
}

/// Task information, used for `sys_task_info`
#[derive(Default, Clone)]
pub struct TaskInfo {
    /// The first time the task is scheduled
    pub first_schedule_time: Option<NonZeroUsize>,
    /// The number of times each syscall is called
    pub syscall_times: BTreeMap<u32, u32>,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    #[allow(unused)]
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }

    /// set the first schedule time, used for `sys_task_info`
    pub fn set_first_schedule_time(&mut self, time: usize) {
        if self.task_info.first_schedule_time.is_none() {
            self.task_info.first_schedule_time = Some(NonZeroUsize::new(time).unwrap());
        }
    }
}

impl TaskControlBlock {
    /// Create a new task
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process), ustack_base, alloc_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc();
        let kstack_top = kstack.get_top();
        Self {
            process: Arc::downgrade(&process),
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                    task_info: TaskInfo::default(),
                    stride: 0,
                    pass: BIG_STRIDE / 16,
                })
            },
        }
    }

    /// increase the number of syscalls, used for `sys_task_info`
    pub fn inc_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner_exclusive_access();
        inner
            .task_info
            .syscall_times
            .entry(syscall_id as u32)
            .and_modify(|e| *e += 1)
            .or_insert(1);
    }

    /// get task statistics info, used for `sys_task_info`
    pub fn get_task_info(&self) -> TaskInfo {
        self.inner_exclusive_access().task_info.clone()
    }

    /// set priority, used for `sys_set_priority`
    pub fn set_priority(&self, prio: usize) {
        let mut inner = self.inner_exclusive_access();
        inner.stride = prio;
        inner.pass = BIG_STRIDE / prio;
    }
}

#[derive(Copy, Clone, PartialEq)]
/// The execution status of the current process
pub enum TaskStatus {
    /// ready to run
    Ready,
    /// running
    Running,
    /// blocked
    Blocked,
}
