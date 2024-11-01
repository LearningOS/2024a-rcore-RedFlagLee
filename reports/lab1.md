## 编程作业

在TaskControlBlock里新增task_start_time和syscall_time成员用于记录任务开始时间和统计系统调用次数，初始化为0和全是0的数组。

封装increase_current_syscall函数用于增加当前任务指定系统调用调用的次数，在trap_handler里执行系统调用前执行。在启动第一个任务时用get_time_ms更新task_start_time。在切换下一个任务时判断如果为0，则代表第一次更新启动时间，用get_time_ms更新task_start_time。

最后在sys_task_info里将返回值从-1改为0即可开始测试。

## 简答作业

1. 访问0x0非法内存地址、在U态非法使用了S态下的指令sret和在U态下非法访问了S态下才能访问的sstatus寄存器。rustsbi版本为0.3.0-alpha.2
2. 
	2.1 a0寄存器的值代表调用__restore时传的参数，为内核栈的栈底地址。\__restore在在系统调用返回或异常处理返回时被调用。
	2.2 TrapContext里的成员分别是x: [usize; 32]、sstatus和sepc，此时内核栈栈底为TrapContext，所以t0里是sstatus，t1是sepc，t2是用户栈地址。sepc用于在sret时返回用户态代码的地址，sstatus存有trap的特权级信息。
3. x2里的用户栈指针保存到了sscratch寄存器里，不需要从内核栈中恢复。x4没有使用，所以跳过恢复。
4. 交换了sp和sscratch寄存器里的值，现在sp指向用户栈，sscratch指向内核栈，为恢复用户态作准备。
5. 执行sret后从S态切换回U态，从sepc存的地址处继续执行。
6. sp指向了内核态，为后面保存寄存器内容作准备。
7. ecall指令
