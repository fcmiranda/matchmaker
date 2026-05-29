use cba::bait::ResultExt;
use cba::unwrap;
use ratatui::text::{Line, Text};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::AppendOnly;

// Images?
#[derive(Debug)]
pub struct Preview {
    lines: AppendOnly<Line<'static>>,
    string: Arc<Mutex<Option<Text<'static>>>>,
    pub image: Arc<Mutex<Option<image::DynamicImage>>>,
    pub changed: Arc<AtomicBool>,
}

impl Preview {
    pub fn results(&self) -> Text<'_> {
        if let Some(s) = unwrap!(self.string.lock().prefix("Previewer panicked")._elog()).as_ref() {
            s.clone()
        } else {
            let output = self.lines.read();
            Text::from_iter(output.iter().map(|(_, line)| line.clone()))
        }
    }

    pub fn len(&self) -> usize {
        if let Some(s) = unwrap!(self.string.lock().prefix("Previewer panicked")._elog()).as_ref() {
            s.height()
        } else {
            let output = self.lines.read();
            output.iter().count()
        }
    }

    pub fn is_empty(&self) -> bool {
        if let Some(s) = unwrap!(self.string.lock().prefix("Previewer panicked")._elog()).as_ref() {
            s.height() == 0
        } else {
            let output = self.lines.read();
            output.iter().next().is_none()
        }
    }

    pub fn changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }

    pub fn new(
        lines: AppendOnly<Line<'static>>,
        string: Arc<Mutex<Option<Text<'static>>>>,
        image: Arc<Mutex<Option<image::DynamicImage>>>,
        changed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            lines,
            string,
            image,
            changed,
        }
    }
}
