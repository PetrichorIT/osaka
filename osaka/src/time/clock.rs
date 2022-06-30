use super::SimTime;

#[derive(Debug, Clone)]
pub struct Clock;

impl Clock {
    pub fn new() -> Clock {
        Clock
    }

    pub fn now() -> SimTime {
        now()
    }
}

pub fn now() -> SimTime {
    SimTime::now()
}
