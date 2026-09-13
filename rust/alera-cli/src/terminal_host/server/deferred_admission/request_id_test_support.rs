use super::DeferredAdmission;

impl DeferredAdmission {
    pub(in crate::terminal_host::server) fn request_ids_for_test(
        &self,
        request_type: &str,
    ) -> Vec<Option<i64>> {
        let state = self.inner.lock_state();
        state
            .active
            .values()
            .filter(|job| job.request_type == request_type)
            .map(|job| job.request_id)
            .chain(
                state
                    .queued
                    .iter()
                    .flat_map(|queue| queue.iter())
                    .filter(|job| job.request_type == request_type)
                    .map(|job| job.request_id),
            )
            .chain(
                state
                    .delayed
                    .values()
                    .filter(|job| job.request_type == request_type)
                    .map(|job| job.request_id),
            )
            .collect()
    }
}
