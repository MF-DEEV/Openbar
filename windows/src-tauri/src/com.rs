// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! COM apartments for the section backends. Audio sessions want a non-UI MTA
//! thread (spec 05 §2.4); shell icons and thumbnails want an STA. Each guard
//! uninitializes only what it initialized, so a thread that was already in
//! the other apartment is left alone.

#[cfg(windows)]
pub struct Apartment {
    owned: bool,
}

#[cfg(windows)]
impl Apartment {
    pub fn mta() -> Result<Self, String> {
        Self::enter(windows::Win32::System::Com::COINIT_MULTITHREADED)
    }

    pub fn sta() -> Result<Self, String> {
        Self::enter(
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED
                | windows::Win32::System::Com::COINIT_DISABLE_OLE1DDE,
        )
    }

    fn enter(mode: windows::Win32::System::Com::COINIT) -> Result<Self, String> {
        use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
        let hr = unsafe { windows::Win32::System::Com::CoInitializeEx(None, mode) };
        if hr == RPC_E_CHANGED_MODE {
            // Already in the other apartment: usable for most calls, not ours to undo.
            return Ok(Apartment { owned: false });
        }
        hr.ok().map_err(|e| e.to_string())?;
        Ok(Apartment { owned: true })
    }
}

#[cfg(windows)]
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.owned {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

/// A long-lived thread in one apartment that runs jobs in order. COM objects
/// made by a job never leave its thread.
pub struct Worker {
    tx: std::sync::Mutex<std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>>,
}

impl Worker {
    pub fn spawn(name: &str, sta: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let _ = std::thread::Builder::new().name(name.into()).spawn(move || {
            #[cfg(windows)]
            let _apartment = if sta { Apartment::sta() } else { Apartment::mta() };
            #[cfg(not(windows))]
            let _ = sta;
            for job in rx {
                job();
            }
        });
        Worker {
            tx: std::sync::Mutex::new(tx),
        }
    }

    /// Runs `job` on the worker and waits for its result.
    pub fn run<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        let sent = self.tx.lock().unwrap().send(Box::new(move || {
            let _ = tx.send(job());
        }));
        sent.ok()?;
        rx.recv().ok()
    }

    /// Queues `job` without waiting: COM callbacks hand their work over here.
    pub fn post(&self, job: impl FnOnce() + Send + 'static) {
        let _ = self.tx.lock().unwrap().send(Box::new(job));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_run_in_order_on_one_thread() {
        let worker = Worker::spawn("test-worker", false);
        let a = worker.run(|| std::thread::current().id()).unwrap();
        let b = worker.run(|| std::thread::current().id()).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, std::thread::current().id());
        assert_eq!(worker.run(|| 2 + 2), Some(4));
    }
}
