use alloc::sync::Arc;
use core::cell::UnsafeCell;

use crate::arch::{instructions, ArchPerCpu};
use crate::config::MAX_CPUS;
use crate::sync::LazyInit;
use crate::task::{CurrentTask, Task};

static CPUS: [LazyInit<PerCpu>; MAX_CPUS] = [LazyInit::new(); MAX_CPUS];

pub struct PerCpuData<T> {
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for PerCpuData<T> {}

impl<T> PerCpuData<T> {
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    pub unsafe fn as_ref(&self) -> &T {
        &*self.data.get()
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn as_mut(&self) -> &mut T {
        &mut *self.data.get()
    }
}

/// Each CPU can only accesses its own `PerCpu` instance.
#[repr(C)]
pub struct PerCpu {
    self_vaddr: usize,
    id: usize,
    idle_task: Arc<Task>,
    current_task: PerCpuData<Arc<Task>>,
    arch: PerCpuData<ArchPerCpu>,
}

impl PerCpu {
    fn new(id: usize) -> Self {
        let idle_task = Task::new_idle();
        Self {
            self_vaddr: &CPUS[id] as *const _ as usize,
            id,
            current_task: PerCpuData::new(idle_task.clone()),
            idle_task,
            arch: PerCpuData::new(ArchPerCpu::new()),
        }
    }

    pub fn current<'a>() -> &'a Self {
        unsafe { &*(instructions::thread_pointer() as *const Self) }
    }

    pub fn current_cpu_id() -> usize {
        Self::current().id
    }

    pub const fn idle_task(&self) -> &Arc<Task> {
        &self.idle_task
    }

    pub fn current_task(&self) -> CurrentTask {
        // Safety: Even if there is an interrupt and task preemption after
        // calling this method, the reference of percpu data (e.g., `current_task`) can keep unchanged
        // since it will be restored after context switches.
        CurrentTask(unsafe { self.current_task.as_ref() })
    }

    pub unsafe fn set_current_task(&self, task: Arc<Task>) {
        // We must disable interrupts and task preemption when update this field.
        assert!(instructions::irqs_disabled());
        let old_task = core::mem::replace(self.current_task.as_mut(), task);
        drop(old_task)
    }

    pub const fn arch_data(&self) -> &PerCpuData<ArchPerCpu> {
        &self.arch
    }
}

pub fn init_percpu() {
    let cpu_id = 0;
    let mut percpu = PerCpu::new(cpu_id);
    percpu.arch.get_mut().init(cpu_id);
    CPUS[cpu_id].init_by(percpu);
    unsafe { instructions::set_thread_pointer(CPUS[cpu_id].self_vaddr) };
}
