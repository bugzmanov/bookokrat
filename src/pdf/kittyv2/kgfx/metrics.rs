use std::sync::atomic::{AtomicUsize, Ordering};

use log::warn;

use super::tracker::HARD_LIMIT;

static SHM_CREATED: AtomicUsize = AtomicUsize::new(0);
static SHM_UNLINKED: AtomicUsize = AtomicUsize::new(0);
static SHM_UNLINK_ERRORS: AtomicUsize = AtomicUsize::new(0);

fn audit_live_shm() {
    let created = SHM_CREATED.load(Ordering::Relaxed);
    let unlinked = SHM_UNLINKED.load(Ordering::Relaxed);
    let live = created.saturating_sub(unlinked);
    let threshold = (HARD_LIMIT * 3) / 2;

    if live > threshold {
        if cfg!(debug_assertions) && !std::thread::panicking() {
            panic!(
                "SHM leak audit failed: live_estimate={live}, threshold={threshold}, created={created}, unlinked={unlinked}, unlink_errors={}",
                SHM_UNLINK_ERRORS.load(Ordering::Relaxed)
            );
        } else {
            warn!(
                "SHM leak audit warning: live_estimate={} threshold={} created={} unlinked={} unlink_errors={}",
                live,
                threshold,
                created,
                unlinked,
                SHM_UNLINK_ERRORS.load(Ordering::Relaxed)
            );
        }
    }
}

pub(crate) fn record_shm_create() {
    SHM_CREATED.fetch_add(1, Ordering::Relaxed);
    audit_live_shm();
}

pub(crate) fn record_shm_unlink_success() {
    SHM_UNLINKED.fetch_add(1, Ordering::Relaxed);
    audit_live_shm();
}

pub(crate) fn record_shm_unlink_error() {
    SHM_UNLINK_ERRORS.fetch_add(1, Ordering::Relaxed);
    audit_live_shm();
}
