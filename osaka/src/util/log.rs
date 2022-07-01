use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::util::{atomic_cell::AtomicCell, SyncWrapper};

#[doc(hidden)]
pub static INDENT: SyncWrapper<RefCell<String>> = SyncWrapper::new(RefCell::new(String::new()));
pub static LOG_SCOPES: AtomicBool = AtomicBool::new(false);

pub struct ScopeGuard {
    name: &'static str,
}

impl ScopeGuard {
    pub fn new(name: &'static str) -> Self {
        if LOG_SCOPES.load(Ordering::Relaxed) {
            println!("{}<{} @start>", INDENT.get_ref().borrow(), name);
            INDENT.get_ref().borrow_mut().push(' ');
        }

        Self { name }
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if LOG_SCOPES.load(Ordering::Relaxed) {
            INDENT.get_ref().borrow_mut().pop();
            println!("{}<{} @end>", INDENT.get_ref().borrow(), self.name);
        }
    }
}

#[macro_export]
macro_rules! scope {
    ($name: expr => $e: expr) => {{
        let _guard = $crate::util::ScopeGuard::new($name);
        let ret = (|| $e)();
        drop(_guard);
        ret
    }};
}

#[macro_export]
macro_rules! tprintln {
    () => {{
        println!()
    }};
    ($($arg:tt)*) => {
        println!("{}[{}] {}", $crate::util::INDENT.get_ref().borrow(),  $crate::time::SimTime::now(), format!($($arg)*))
    };
}

#[macro_export]
macro_rules! tprint {
    () => {{
        print!()
    }};
    ($($arg:tt)*) => {
        print!("[{}], {}", $crate::util::INDENT.get_ref().borrow(), $crate::time::SimTime::now(), format!($($arg)*))
    };
}
