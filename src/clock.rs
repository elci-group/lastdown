/// Deterministic clock. Simulations advance it. Wall time is not consulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualClock {
    now_ms: u64,
}

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self { now_ms }
    }

    pub fn now(&self) -> u64 {
        self.now_ms
    }

    pub fn advance(&mut self, ms: u64) {
        self.now_ms = self.now_ms.saturating_add(ms);
    }
}
