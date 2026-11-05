pub(crate) fn dispatch<T>(pending: &mut Option<T>, attempt: impl FnOnce(T) -> Option<T>) {
    if let Some(request) = pending.take() {
        *pending = attempt(request);
    }
}
