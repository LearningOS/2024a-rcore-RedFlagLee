use alloc::vec;
use alloc::vec::Vec;

/// Deadlock detection
pub struct DeadLockDetect {
    /// Available resources
    pub available: Vec<u32>,
    /// Allocated resources
    pub allocation: Vec<Vec<u32>>,
    /// Needed resources
    pub need: Vec<Vec<u32>>,
}

impl DeadLockDetect {
    /// Create a new deadlock detection
    pub fn new(threads: u32) -> Self {
        DeadLockDetect {
            available: Vec::new(),
            allocation: vec![vec![]; threads as usize],
            need: vec![vec![]; threads as usize],
        }
    }

    /// try to detect deadlock
    pub fn detect(&self) -> bool {
        let mut work = self.available.clone();
        let mut finish = vec![false; self.allocation.len()];

        loop {
            let mut can_finish = false;
            for i in 0..self.allocation.len() {
                if finish[i] {
                    continue;
                }
                let mut can_alloc = true;
                for j in 0..self.need[i].len() {
                    if self.need[i][j] > work[j] {
                        can_alloc = false;
                        break;
                    }
                }
                if can_alloc {
                    for j in 0..self.need[i].len() {
                        work[j] += self.allocation[i][j];
                    }
                    finish[i] = true;
                    can_finish = true;
                }
            }
            if !can_finish {
                break;
            }
        }

        !finish.iter().all(|&x| x)
    }
}
