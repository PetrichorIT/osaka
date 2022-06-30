use std::future::Future;

pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
    let mut e = crate::runtime::enter::enter(false);
    e.block_on(f).unwrap()
}
