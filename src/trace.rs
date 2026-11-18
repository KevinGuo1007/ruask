//! Opt-in monotonic milestones. Never record query text, URLs, headers or credentials.
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::time::Instant;

use serde::Serialize;

pub struct Trace {
    start: Option<Instant>,
    events: RefCell<Vec<Event>>,
}

#[derive(Serialize)]
struct Event {
    name: &'static str,
    elapsed_us: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
}

impl Trace {
    pub fn new() -> Self {
        Self {
            start: (std::env::var_os("RUASK_TRACE").as_deref() == Some(std::ffi::OsStr::new("1")))
                .then(Instant::now),
            events: RefCell::new(Vec::new()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.start.is_some()
    }

    pub fn mark(&self, name: &'static str) {
        self.record(name, None);
    }

    pub fn value(&self, name: &'static str, value: u64) {
        self.record(name, Some(value));
    }

    fn record(&self, name: &'static str, value: Option<u64>) {
        if let Some(start) = self.start {
            self.events.borrow_mut().push(Event {
                name,
                elapsed_us: start.elapsed().as_micros(),
                value,
            });
        }
    }

    pub fn once(&self, name: &'static str) {
        if self.start.is_some() && !self.events.borrow().iter().any(|event| event.name == name) {
            self.mark(name);
        }
    }

    pub fn reader<R: Read>(&self, reader: R) -> BodyReader<'_, R> {
        BodyReader {
            reader,
            trace: self,
        }
    }

    pub fn finish(&self, success: bool) {
        if self.start.is_some() {
            self.value("exit", u64::from(success));
            // A closed diagnostic sink must not change the CLI's result.
            let _ = writeln!(
                io::stderr().lock(),
                "ruask-trace: {}",
                serde_json::json!({"version":1,"events":*self.events.borrow()})
            );
        }
    }
}

pub struct BodyReader<'a, R> {
    reader: R,
    trace: &'a Trace,
}

impl<R: Read> Read for BodyReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(buffer)?;
        if count > 0 {
            self.trace.once("first_http_body_byte");
        }
        Ok(count)
    }
}
