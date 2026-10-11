//! File work may pause independently while an Android push wake checks mail.
use super::*;
impl Engine {
    /// The platform owns this gate. Never turn a lifecycle pause into user cancel.
    pub fn set_files_paused(&self, on: bool) {
        self.files_paused.send_if_modified(|current| {
            if *current == on {
                false
            } else {
                *current = on;
                true
            }
        });
        self.queue_changed.notify_one();
    }
    pub fn file_work_paused(&self) -> bool {
        *self.files_paused.borrow()
            || *self.suspended.borrow()
            || self.stopped.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Only wrap network work or waiting for a slot. Filesystem publication must
    /// finish under its durable intent even if the platform pauses meanwhile.
    pub(super) async fn file_request<T>(
        &self,
        work: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let mut files = self.files_paused.subscribe();
        let mut all = self.suspended.subscribe();
        if *files.borrow_and_update()
            || *all.borrow_and_update()
            || self.stopped.load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(Error::FilePaused);
        }
        tokio::select! {
            biased;
            _=files.changed()=>Err(Error::FilePaused),
            _=all.changed()=>Err(Error::FilePaused),
            result=work=>result,
        }
    }
}
