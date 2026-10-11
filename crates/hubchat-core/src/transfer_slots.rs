//! App-wide file slots. Waiting for one hub never reserves a global slot.
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;
#[derive(Default)]
pub(crate) struct TransferSlots {
    hubs: Mutex<HashSet<String>>,
    pub changed: Notify,
}
pub(crate) struct Permit {
    slots: Arc<TransferSlots>,
    hub: String,
}
impl TransferSlots {
    pub fn try_acquire(self: &Arc<Self>, hub: &str) -> Option<Permit> {
        let mut hubs = self.hubs.lock().unwrap();
        if hubs.len() >= 2 || hubs.contains(hub) {
            return None;
        }
        hubs.insert(hub.into());
        Some(Permit {
            slots: self.clone(),
            hub: hub.into(),
        })
    }
    pub async fn acquire(self: &Arc<Self>, hub: &str) -> Permit {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if let Some(permit) = self.try_acquire(hub) {
                return permit;
            }
            changed.await;
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.slots.hubs.lock().unwrap().remove(&self.hub);
        self.slots.changed.notify_waiters();
    }
}
