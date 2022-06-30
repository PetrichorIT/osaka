#[macro_export]
macro_rules! tprintln {
    () => {{
        println!()
    }};
    ($($arg:tt)*) => {
        println!("[{}], {}", $crate::time::SimTime::now(), format!($($arg)*))
    };
}

#[macro_export]
macro_rules! tprint {
    () => {{
        print!()
    }};
    ($($arg:tt)*) => {
        print!("[{}], {}", $crate::time::SimTime::now(), format!($($arg)*))
    };
}
