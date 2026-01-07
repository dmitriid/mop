use chrono::{DateTime, Local};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogCategory {
    Net,
    Disc,
    Soap,
    Http,
    Xml,
    App,
}

impl LogCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogCategory::Net => "NET",
            LogCategory::Disc => "DISC",
            LogCategory::Soap => "SOAP",
            LogCategory::Http => "HTTP",
            LogCategory::Xml => "XML",
            LogCategory::App => "APP",
        }
    }

    fn from_target(target: &str) -> Self {
        let target_lower = target.to_lowercase();
        if target_lower.contains("net") || target_lower.contains("socket") || target_lower.contains("multicast") {
            LogCategory::Net
        } else if target_lower.contains("upnp") || target_lower.contains("disc") || target_lower.contains("rupnp") || target_lower.contains("ssdp") {
            LogCategory::Disc
        } else if target_lower.contains("soap") {
            LogCategory::Soap
        } else if target_lower.contains("http") || target_lower.contains("reqwest") {
            LogCategory::Http
        } else if target_lower.contains("xml") || target_lower.contains("quick_xml") {
            LogCategory::Xml
        } else {
            LogCategory::App
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogSeverity {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogSeverity::Error => "ERROR",
            LogSeverity::Warn => "WARN",
            LogSeverity::Info => "INFO",
            LogSeverity::Debug => "DEBUG",
            LogSeverity::Trace => "TRACE",
        }
    }
}

impl From<log::Level> for LogSeverity {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogSeverity::Error,
            log::Level::Warn => LogSeverity::Warn,
            log::Level::Info => LogSeverity::Info,
            log::Level::Debug => LogSeverity::Debug,
            log::Level::Trace => LogSeverity::Trace,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub category: LogCategory,
    pub severity: LogSeverity,
    pub message: String,
}

impl LogEntry {
    pub fn format_line(&self) -> String {
        format!(
            "{} [{}] {}",
            self.timestamp.format("%H:%M:%S"),
            self.category.as_str(),
            self.message
        )
    }

    pub fn format_export_line(&self) -> String {
        format!(
            "{} [{}] {:5} {}",
            self.timestamp.format("%H:%M:%S"),
            self.category.as_str(),
            self.severity.as_str(),
            self.message
        )
    }
}

pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

pub const LOG_BUFFER_CAPACITY: usize = 2000;

pub struct RingBufferLogger {
    buffer: LogBuffer,
}

impl RingBufferLogger {
    pub fn new() -> (Self, LogBuffer) {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_BUFFER_CAPACITY)));
        let buffer_handle = Arc::clone(&buffer);
        (Self { buffer }, buffer_handle)
    }
}

impl log::Log for RingBufferLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Trace
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let entry = LogEntry {
            timestamp: Local::now(),
            category: LogCategory::from_target(record.target()),
            severity: LogSeverity::from(record.level()),
            message: record.args().to_string(),
        };

        if let Ok(mut buffer) = self.buffer.lock() {
            if buffer.len() >= LOG_BUFFER_CAPACITY {
                buffer.pop_front();
            }
            buffer.push_back(entry);
        }
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<RingBufferLogger> = OnceLock::new();

pub fn init_logger() -> LogBuffer {
    let (logger, buffer) = RingBufferLogger::new();

    if LOGGER.set(logger).is_ok() {
        if let Some(logger) = LOGGER.get() {
            log::set_logger(logger).expect("Failed to set logger");
            log::set_max_level(log::LevelFilter::Trace);
        }
    }

    buffer
}
